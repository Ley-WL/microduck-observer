# 倒地起身结束仍倒地却回HOME（2026-10-11）

用户反馈倒地起身失败，倒地时预备姿势仍使能。仅读取已有数据，无代发动作。

最新getup-session eventTime1791721535100，共300帧全部getup/a481；初始89.975°，末模型92.303°，单帧最小1.156°不代表持续站稳，结束事件94.7005°。起身窗口6秒结束后原程序继续native HOME，所有关节达目标±5°、result holding、recoveryUprightAtEnd=false；这证实关节到HOME≠躯干直立。后续failure1791721612547属于更晚home-transition（#23误差−5.27°），不是同一场起身，禁止混用。

首次snapshot读取时正在运行新的walk session、15torque1；用户随后说明未使能，重新只读取得before-deploy-snapshot（关节样本age4.45ms）确认disabled/15torque0，最新回读覆盖前态，未自动停机。

修复保留模型六秒窗口，结束时按现有60°倒地判据检查：仍倒地则记录recovery-failed/停止推理卸力，禁止继续HOME。网页模型HOME、App预备、策略初始HOME、起身后的HOME共用姿态预检查；倒地时拒绝目标及使能写入，HOME过渡中再倾倒同样中止卸力。新增检查使用现有标定及同一gravity变换，不重新归零或更改IMU；零位和普通手动单关节不改变。

这是对kissqy无条件窗口到期→HOME的明确平台失败处理补充，不伪称供体原本有此保护；原切换时序/history/其它PD均保留，未训练或替换权重。新增94.7°实证回归与倒地预备零写入Mock回归，129检查通过/1外部fixture忽略，ARM构建成功。软件/只读部署不是起身成功实机验收。

部署完成runtime-20261011-203144，前版201917可回滚，二进制SHAa7e69bd4d2c8d6661ee636882227c3ff06bce079215f1aaacedfa4b903f759d4。候选999/999完整关节帧/20生产快照卸力，网页与模型逐文件diff相同、设置hash保持；未代发动作，不保证模型起身能力。
