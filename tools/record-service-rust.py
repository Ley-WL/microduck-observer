"""Reconcile installed Rust helpers with evidence and current physical records."""
from pathlib import Path
import hashlib,json

root=Path(__file__).resolve().parents[1]
evidence=root/'docs/实测记录/附件/平台/2026-10-11/service-rust'
deployment=json.loads((evidence/'deployment.json').read_text(encoding='utf-8'))
assert deployment['installed'] and deployment['netplanUnchanged'] and deployment['calibrationUnchanged']
assert len(deployment['allDisabledSnapshots'])==20
for name,digest in deployment['binaryHashes'].items():
    assert hashlib.sha256((root/'backend-rust/target/aarch64-unknown-linux-gnu/release'/name).read_bytes()).hexdigest()==digest
logs=(evidence/'tests.log').read_text(encoding='utf-8')
for summary in ['107 passed; 0 failed; 1 ignored','6 passed; 0 failed','1 passed; 0 failed']:assert summary in logs
note='> **2026-10-11 板上机器人服务全Rust迁移（已部署）：** 用户要求除一次性脚本外机器人服务尤其主板运行部分全部Rust。Wi-Fi管理和受限电源辅助已改为Rust src/bin/ducklink-wifi.rs、microduck-power-helper.rs，安装独立/usr/local/lib/microduck-services/20261011-service-rust-v1，主Observer仍原kissqy-native-v2进程未重启/升权。Wi-Fi原Unix socket/peer UID验证、异步连接/关联+IPv4后保存、netplan原子更新及生成失败字节回滚保留；密码仅走wpa控制socket，不进argv/日志/错误。电源只接受两个固定动作，仍systemd延迟5秒；原活动Python辅助源删除并归档回滚，仓库deploy/power-helper.py删除，打包包含ARM helper。114Rust测试通过/1忽略，含Unix传输清理、标准PSK向量、输入校验、错误隐私、生成失败配置回滚和权限测试；ARM构建通过。板上Wi-Fi关联和netplan字节未改、模型/主二进制/标定/持久增益/BLE身份保持，20快照15舵机在线卸力零missing，两个服务active，电源模板ExecStart为Rust，进程检查无Python；tofd/rsyslog仍masked。首次SSH超时未执行，重试因动作中预检拒绝；只读确认disabled后安装，用户随后回复已停止。无训练/运动/实际切网/关机重启，真实网络切换及电源动作尚未现场验收。系统netplan工具不属于机器人服务；本机和部署/诊断/实测附件的一次性Python脚本保留。证据附件/平台/2026-10-11/service-rust/{deployment,source-hashes}.json及tests.log，回滚目录/var/lib/microduck-observer/service-rust-backup-20261011。\n\n'
for name in ['AGENTS.md','docs/实测记录/03-舵机实测.md','docs/实测记录/04-网络与观测平台.md','docs/实测记录/README.md']:
    path=root/name;old=path.read_text(encoding='utf-8')
    if '2026-10-11 板上机器人服务全Rust迁移（已部署）' not in old:path.write_text(note+old,encoding='utf-8')
paths=[root/name for name in ['backend-rust/Cargo.toml','backend-rust/Cargo.lock','backend-rust/src/bin/ducklink-wifi.rs','backend-rust/src/bin/microduck-power-helper.rs','deploy/install-service-rust.py','tools/deploy-service-rust.py','tools/record-service-rust.py','deploy/package_rust.py']]
(evidence/'source-hashes.json').write_text(json.dumps({p.relative_to(root).as_posix():hashlib.sha256(p.read_bytes()).hexdigest() for p in paths},indent=2),encoding='utf-8')
(evidence/'evidence-hashes.json').write_text(json.dumps({p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in evidence.iterdir() if p.is_file() and p.name!='evidence-hashes.json'},indent=2),encoding='utf-8')
print('Installed helpers and 114 passing tests verified; four records updated.')
