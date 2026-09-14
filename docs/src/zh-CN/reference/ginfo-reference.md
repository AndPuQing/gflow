# ginfo 参考

`ginfo` 用于查看调度器与 GPU 分配信息。

如果某张 GPU 被非 gflow 的计算进程占用，它会以 `unmanaged(pid=…)` 的原因显示为 allocated，并且 gflow 会在它空闲前一直不分配这张卡。`ginfo` 还会在表格下方列出这些进程的内存、利用率、存活时长，以及释放该 GPU 的准确命令。

## 用法

```bash
ginfo
```

## 示例

```bash
ginfo
watch -n 2 ginfo
```

## 输出

上方表格与 `sinfo` 类似：分区、GPU 数量、节点（GPU）索引、状态，以及占用每张卡的作业或原因。

```
PARTITION  GPUS  NODES  STATE      JOB(REASON)
gpu        1     0      allocated  (unmanaged(pid=3471817))
gpu        1     1      idle
```

下方会列出被非 gflow 进程占用、需要人工处理的 GPU 及其细节：

```
Non-gflow GPU processes (gflow will not allocate a GPU while these are attached):
GPU  PID      GPU MEM  UTIL  AGE  NOTE           RELEASE
gpu  3471817  642M     0%    48h  idle leftover  gctl gpu-process ignore --gpu 0 --pid 3471817
Release a card without touching the process: run the RELEASE command for that PID.
```

- `GPU MEM` / `UTIL` 来自 NVML；显示 `?` 表示驱动没有上报该值。
- `AGE` 是宿主机进程的存活时长（仅 Linux）。
- `NOTE` 为 `idle leftover` 时，表示该进程占用 GPU 超过一小时却几乎没有执行任何 kernel，通常是被遗留下来的空转进程而不是真实作业。
- `RELEASE` 是让调度器重新使用该 GPU 的命令，详见 [gctl 参考](./gctl-reference)。

当前生效的「仅运行时」忽略规则会列在最后，并附带对应的 `unignore` 命令：

```
Active GPU process ignore overrides (runtime-only; cleared when gflowd restarts):
  gpu=0 pid=3471817  (undo: gctl gpu-process unignore --gpu 0 --pid 3471817)
```

## 选项

- `-v/-vv/-q`：调整日志输出级别
- `--config <path>`：指定配置文件（隐藏选项）
