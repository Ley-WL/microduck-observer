# Observer 模型

## 金鸡独立模型的区别

本次自行训练的模型在 [candidates/rear_hook_v62b/](candidates/rear_hook_v62b/)，采用 **复刻项目 STL/MJCF + HD1910（1910 M6）执行器参数**，使用 v53 第 500 轮权重与 v62b 等待时钟。它是候选归档，开发集完整周期 242/256（94.53%），未通过验收，当前 observer 尚未支持其完整时钟合同，不自动加载。

Hugging Face 的 [RemiFabre/microduck-flamingo-cycle](https://huggingface.co/RemiFabre/microduck-flamingo-cycle) 是另一个上游发布模型，采用 XL330 配置。下载该模型或克隆上游 RL 仓库，不会获得本地训练的 HD1910 v62b 权重。

具体模型身份、权重 SHA256、执行合同和验证边界以各模型的 metadata 及说明为准。
