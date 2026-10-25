# HD1910 轮滑 30001

| 模型 | 更新数 | 用途 | 状态 |
|---|---:|---|---|
| [swizzle_hd1910.onnx](swizzle_hd1910.onnx) | 30001 | 四被动轮前后滑行与转向 | 本地接入，未部署实测 |

基于 HD1910 参数和 Observer 关节限位重新训练；保留独立轮滑模型，不替换行走模型。卸力后回中长按 LB＋RB 1 秒进入装轮预备，扶稳装轮后按 A 平衡。

[控制流程、训练路径与测试结论](../docs/swizzle-controller-20261025.md) · [执行合同](swizzle_hd1910.metadata.json)
