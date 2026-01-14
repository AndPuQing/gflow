# ginfo 命令参考

完整的 `ginfo` 命令参考 — gflow 的调度器检查工具。

## 概要

```bash
ginfo <COMMAND> [OPTIONS]
```

## 描述

`ginfo` 连接到运行中的 gflow 守护进程（`gflowd`）并打印调度器元数据、GPU 可用性以及当前绑定到每个设备的任务。该命令是只读的，可以根据需要频繁运行来监控系统。

如果守护进程无法访问，`ginfo` 会报告连接失败，而不修改任何状态。

## 命令

### `info`

显示当前调度器状态和 GPU 分配。

**语法**
```bash
ginfo
```

**显示内容**
- GPU 索引、短 UUID 和可用性
- 当前占用每个 GPU 的任务（任务 ID 和运行名称）
- 调度器元数据（检测到的总 GPU 数、可用性）

**示例**
```bash
# 使用默认配置路径查询默认守护进程
ginfo

# 使用自定义配置文件
ginfo --config ~/gflow-dev/config.toml info

# 每 2 秒刷新一次视图
watch -n 2 ginfo
```

当守护进程离线时，该命令打印有用的错误消息，例如：
```
ginfo: daemon not reachable: ...
```

## 全局选项

### `--config <PATH>`

指定连接到守护进程时的备用配置文件。

```bash
ginfo --config /path/to/custom.toml info
```

在运行多个 gflow 实例或测试非默认设置时使用此选项。

### `-v`, `-vv`, `-q`

调整日志详细程度以进行故障排除：
- `-v` 启用信息级日志
- `-vv` 启用调试日志
- `-q` 抑制非错误输出

## 使用模式

```bash
# 与 gqueue 结合获得完整快照
ginfo && gqueue -s Running -f JOBID,NAME,NODES,NODELIST

# 创建轻量级仪表板
watch -n 5 '
  clear
  date
  echo
  ginfo
'
```

`ginfo` 可以安全地从脚本、cron 任务和监控工具运行，因为它永远不会改变调度器状态。
