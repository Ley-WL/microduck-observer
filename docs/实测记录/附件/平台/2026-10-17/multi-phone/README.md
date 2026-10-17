# 多手机绑定 / 2026-10-17

用户确认：多部手机绑定同一只鸭子，轮流连接；随后明确只改代码。本轮无App修改/OTA、无主板访问/部署或真实控制动作。

变更为ducklink_protocol.rs多手机独立凭据与legacy兼容、ducklink.rs单peer和订阅防抢占。绑定保存于成功认证后，解绑仅当前client，原身份与密钥不变。

本机WSL Ubuntu-24.04执行cargo test --locked --bin microduck-observer：193通过，1忽略（test.log）。补充旧owner解绑与订阅交接测试后运行ducklink过滤测试30通过（focused-test.log）。已验证模拟加密轮换A/B/C/A、已绑定/未绑定第二地址拒绝、重复通知不清当前会话、重启持久化和独立解绑。并非两部真实手机验收。

原远程构建通道8422超时，已终止该传输，改用本机WSL完成检查；build.sh只是远程构建模板，本轮未完成ARM编译。source.tar.gz为最终源码快照，不是可部署包。用户未授权本轮部署，设备现行服务不变。
