# 当前实体手柄轮滑转向复跑（2026-10-29）

本轮实际重跑确认：当前模型能在前进/倒滑中稳定左右转弯，原地右转存在显著弱响应。

模型SHA539e6b2e7e4c7603ddd35ef27bc06742b292f9f3eede567f82111c0975375160，与110644部署留存模型bank一致。冻结模型，复用HD1910四被动轮/Observer限位源，342源哈希检查通过，无训练、模型修改、部署或硬件动作。

手柄每轴独立映射，原始死区0.15后线性重映射，推力=-ly×0.4、转向意图=-lx×0.25；实际驱动flat更大时半杆映射会不同，满杆不受此影响。满斜推为0.4/±0.25，不使用网页圆形归一化。Heading最大目标航向变化率0.5rad/s，模型输入夹紧±0.5rad，抗积分饱和与松杆锁定当前航向保持，固定20ms。

10工况×新seed291001/291007×8world共160例；每例2秒准备、10秒输入、4秒松杆。160/160未触发done/timeout或45°倾倒门，未倒不等于转向合格。

| 工况 | 10秒平均转角 | 16例范围 |
|---|---:|---:|
| 原地左满杆 | 左243.8° | 左214.1～270.4° |
| 原地右满杆 | 右135.7° | 右9.0～215.9° |
| 原地左半杆 | 左96.2° | 左84.9～107.4° |
| 原地右半杆 | 右58.5° | 右7.0～91.6° |
| 前进左满杆 | 左279.6° | 左271.4～288.8° |
| 前进右满杆 | 右284.2° | 右278.9～291.2° |
| 倒滑左满杆 | 左283.4° | 左271.8～288.3° |
| 倒滑右满杆 | 右283.0° | 右273.0～288.2° |

原地右满杆2/16、右半杆5/16转角不足30°。前进左右平均角速度+0.501/-0.486rad/s，倒滑+0.504/-0.507rad/s，接近满杆0.5rad/s目标。直行10秒平均右偏4.3°；前进左右转松杆4秒平均仍滑移0.82/0.71m，非主动刹停。

边界：Python等价控制律、理想名义IMU与BAM，非ARM UART/编码器量化/实测PD及真实地面轮架回放；跨工况不保证相同初态。支持原地右弱在仿真复现，不证明实机全部成因。未改现役模型和手柄参数。

服务器172.29.101.158，GPU5 RTX4090，目录/CodeRepo/Code/leiwu30/swizzle-current-gamepad-20261029，job.exit=0。复用.venv，无依赖安装；Python3.12.3/Torch2.9.1+cu128/ORT1.24.4/MuJoCo3.10.0。评估主体55.02秒（不含导入启动等待），Python进程峰值RSS2.55GiB，非纯推理内存。未新增单次推理/复杂度测试。

```bash
CUDA_VISIBLE_DEVICES=5 PYTHONPATH=/CodeRepo/Code/leiwu30/hd1910-swizzle-observer-20261024/src /CodeRepo/Code/leiwu30/1_code/microduck-training/microduck-rl/.venv/bin/python -u run_current.py --source /CodeRepo/Code/leiwu30/hd1910-swizzle-observer-20261024 --policy /CodeRepo/Code/leiwu30/hd1910-swizzle-observer-20261024/evidence/model30001/policy.onnx --output <新的结果目录>
```

`实测记录/附件/平台/2026-10-29/swizzle-current-simulation/results.json`（本地原始证据） · [运行统计](实测记录/附件/平台/2026-10-29/swizzle-current-simulation/runtime.json)。同目录evidence.tar.gz含脚本/日志/逐帧轨迹，下载SHA d07450865f2d7da533e4aabbe1549408db618d5542c6b9b6641fec57bec137ae核验通过。新增脚本和本报告UTF-8无BOM、LF；环境与临时目录未加入版本控制。
