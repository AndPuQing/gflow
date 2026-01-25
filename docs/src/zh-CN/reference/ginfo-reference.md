# ginfo 参考

`ginfo` 用于查看调度器与 GPU 分配信息。

如果某张 GPU 被非 gflow 的计算进程占用，可能会显示类似 `Unmanaged` 的原因，并且 gflow 会在它空闲前一直不分配这张卡。

## 用法

```bash
ginfo
ginfo completion <shell>
```

## 示例

```bash
ginfo
watch -n 2 ginfo
```

## 选项

- `-v/-vv/-q`：调整日志输出级别
- `--config <path>`：指定配置文件（隐藏选项）
