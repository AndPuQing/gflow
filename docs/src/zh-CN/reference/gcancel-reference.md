# gcancel 参考

`gcancel` 用于取消排队/运行中的任务。

## 用法

```bash
gcancel [--dry-run] <job_ids>
gcancel completion <shell>
```

`<job_ids>` 支持：

- 单个：`42`
- 逗号分隔：`1,2,3`
- 范围：`1-5`
- 混合：`1,3,5-7,10`

## 示例

```bash
gcancel 42
gcancel 1,2,3
gcancel 10-20
```

### 预览（Dry Run）

预览哪些任务可以被取消，以及依赖它们的排队/暂停任务：

```bash
gcancel --dry-run 42
```

