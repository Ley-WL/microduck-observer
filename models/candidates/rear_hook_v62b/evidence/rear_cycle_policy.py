"""Three-second rear-hook clock and frozen-reference balance residual."""
import weakref
import torch
from torch import nn
from torch.distributions import Normal
from rsl_rl.modules import GaussianDistribution
from single_leg_consistent_head import ConsistentHeadMLP

def smooth(x):
    x=x.clamp(0.,1.)
    return x*x*x*(10.-15.*x+6.*x*x)

class CycleClock:
    IDLE,ENTER,HOLD,EXIT,SETTLE=range(5)
    def __init__(self,n,device='cpu',dt=.02):
        self.dt=dt;self.hold_steps=round(3./dt);self.enter_limit=round(3./dt)
        self.stage=torch.zeros(n,device=device,dtype=torch.long)
        self.progress=torch.zeros(n,device=device)
        for key in ['age','hold_ticks','good_ticks','good_max','settle_ticks','stand_run']:
            setattr(self,key,torch.zeros_like(self.stage))
        for key in ['held_full','success','cancelled','aborted','just_success','just_abort']:
            setattr(self,key,torch.zeros(n,device=device,dtype=torch.bool))
    @property
    def enabled(self):return (self.stage==self.ENTER)|(self.stage==self.HOLD)
    @property
    def done(self):return (self.stage==self.SETTLE)&(self.settle_ticks>=round(2./self.dt))
    def reset(self,ids):
        for key,value in vars(self).items():
            if isinstance(value,torch.Tensor):value[ids]=0
    def trigger(self,ids=slice(None)):
        previous=self.progress[ids].clone()
        self.reset(ids);self.progress[ids]=previous;self.stage[ids]=self.ENTER
    def cancel(self,ids=slice(None)):
        active=self.stage[ids]!=self.IDLE
        self.stage[ids]=torch.where(active,torch.full_like(self.stage[ids],self.EXIT),self.stage[ids])
        self.cancelled[ids]|=active
    def step(self,pose_good,standing,dt=None):
        dt=self.dt if dt is None else dt
        self.just_success.zero_();self.just_abort.zero_()
        if dt<=0:return
        if abs(dt-self.dt)>1e-8:raise ValueError('clock timestep changed')
        old=self.stage.clone();self.age+=(old==self.ENTER).long()
        self.progress+=(self.enabled.float()-self.progress).clamp(-dt/1.5,dt/1.5)
        self.progress=torch.where(self.progress<1e-5,0.,torch.where(self.progress>1-1e-5,1.,self.progress))
        entered=(old==self.ENTER)&(self.progress==1.)&pose_good
        self.stage[entered]=self.HOLD
        timed_out=(old==self.ENTER)&~entered&(self.age>=self.enter_limit)
        self.stage[timed_out]=self.EXIT;self.aborted|=timed_out;self.just_abort|=timed_out
        holding=old==self.HOLD
        self.hold_ticks+=holding.long()
        self.good_ticks=torch.where(holding,torch.where(pose_good,self.good_ticks+1,0),self.good_ticks)
        self.good_max=torch.maximum(self.good_max,self.good_ticks)
        finish=holding&(self.hold_ticks>=self.hold_steps)
        self.held_full[finish]=self.good_max[finish]>=self.hold_steps
        self.stage[finish]=self.EXIT
        landed=(old==self.EXIT)&(self.progress==0.)
        self.stage[landed]=self.SETTLE
        settling=old==self.SETTLE;self.settle_ticks+=settling.long()
        self.stand_run=torch.where(settling,torch.where(standing,self.stand_run+1,0),self.stand_run)
        self.just_success=settling&(self.stand_run>=48)&self.held_full&~self.success&~self.cancelled
        self.success|=self.just_success

class RearCycleMLP(nn.Module):
    def __init__(self,base,normalizer):
        super().__init__()
        for v in base.parameters():v.requires_grad_(False)
        self.base=ConsistentHeadMLP(base,normalizer,.5)
        self.balance=nn.Sequential(nn.Linear(61,64),nn.ELU(),nn.Linear(64,64),nn.ELU(),nn.Linear(64,4))
        nn.init.zeros_(self.balance[-1].weight);nn.init.zeros_(self.balance[-1].bias)
        self.register_buffer('command_mean',normalizer._mean[:,48:51].detach().clone())
        self.register_buffer('command_scale',(normalizer._std[:,48:51]+normalizer.eps).detach().clone())
        self.register_buffer('legacy_height',(.005-normalizer._mean[:,50:51])/(normalizer._std[:,50:51]+normalizer.eps))
        self.register_buffer('right_reference',torch.tensor([-.6218122622341831,.9958828719938275,.9764855100282454],device=normalizer._mean.device))
        self.register_buffer('limits',torch.tensor([.12,.15,.15,.15],device=normalizer._mean.device))
        self.last_scale=None
        self.to(normalizer._mean.device)
    def forward(self,x):
        # Public command height is35mm; canonicalize only the frozen teacher query.
        canonical=torch.cat((x[:,:50],self.legacy_height.expand(len(x),-1),x[:,51:]),-1)
        value=self.base(canonical)
        p=x[:,49:50]*self.command_scale[:,1:2]+self.command_mean[:,1:2]
        transfer=smooth(2*p);lift=smooth(2*p-1)
        transfer=torch.where(p<=1e-5,torch.zeros_like(transfer),transfer)
        lift=torch.where(p<=.5,torch.zeros_like(lift),lift)
        self.last_scale=transfer.detach()
        correction=torch.tanh(self.balance(canonical))*self.limits*transfer
        left=value[:,1:5]+correction+lift*torch.tensor([.3,0.,0.,-.15],device=x.device)
        right=value[:,11:14]*(1-lift)+self.right_reference*lift
        return torch.cat((value[:,:1],left,value[:,5:11],right),-1)

class BalanceGaussian(GaussianDistribution):
    def __init__(self,source,std=.035,floor=1e-4):
        super().__init__(14,init_std=floor);self.source=weakref.ref(source);self.floor=floor
        with torch.no_grad():self.std_param[1:5]=std
        self.std_param.requires_grad_(False)
    def update(self,mean):
        scale=self.source().last_scale
        assert scale is not None and scale.shape==(len(mean),1)
        std=self.std_param.expand_as(mean).clone();std[:,1:5]=self.std_param[1:5]*scale+self.floor
        self._distribution=Normal(mean,std)

def install(actor,exploration_std=.035):
    actor.mlp=RearCycleMLP(actor.mlp,actor.obs_normalizer)
    if actor.distribution is not None:
        actor.distribution.std_param.requires_grad_(False)
        actor.distribution=BalanceGaussian(actor.mlp,exploration_std).to(actor.obs_normalizer._mean.device)
    return actor.mlp

def restore(actor,state):
    actor.load_state_dict({k:v for k,v in state.items() if k.startswith('obs_normalizer.')},strict=False)
    install(actor)
    actor.load_state_dict(state if actor.distribution is not None else {k:v for k,v in state.items() if not k.startswith('distribution.')},strict=True)
