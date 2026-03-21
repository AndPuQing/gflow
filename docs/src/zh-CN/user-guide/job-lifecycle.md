# 任务生命周期

本指南解释了 gflow 中任务的完整生命周期，包括状态转换、状态检查和恢复操作。

## 任务状态

gflow 任务可以处于以下七种状态之一：

| 状态 | 简写 | 描述 |
|------|------|------|
| **Queued** | PD | 任务正在等待运行（等待依赖或资源） |
| **Hold** | H | 任务被用户暂停 |
| **Running** | R | 任务正在执行 |
| **Finished** | CD | 任务成功完成 |
| **Failed** | F | 任务因错误终止 |
| **Cancelled** | CA | 任务被用户或系统取消 |
| **Timeout** | TO | 任务超过时间限制 |

### 状态分类

**活动状态**（任务尚未完成）：
- Queued、Hold、Running

**完成状态**（任务已结束）：
- Finished、Failed、Cancelled、Timeout

## 状态转换图

下图只保留核心状态转换。完成态均为终态。

```mermaid
---
showToolbar: true
---
flowchart LR
    Submit([提交]) --> Queued[Queued]
    Queued -->|可运行| Running[Running]
    Queued -->|暂停| Hold[Hold]
    Queued -->|取消 / 依赖失败| Cancelled[Cancelled]
    Hold -->|释放| Queued
    Hold -->|取消| Cancelled
    Running -->|退出码 0| Finished[Finished]
    Running -->|退出码非 0| Failed[Failed]
    Running -->|取消| Cancelled
    Running -->|超时| Timeout[Timeout]
```

右上角工具栏支持放大、适配、下载和全屏查看。

### 状态转换规则

**从 Queued**：
- → **Running**：当依赖满足且资源可用时
- → **Hold**：用户运行 `gjob hold <job_id>`
- → **Cancelled**：用户运行 `gcancel <job_id>` 或依赖失败（启用自动取消时）

**从 Hold**：
- → **Queued**：用户运行 `gjob release <job_id>`
- → **Cancelled**：用户运行 `gcancel <job_id>`

**从 Running**：
- → **Finished**：任务脚本/命令以代码 0 退出
- → **Failed**：任务脚本/命令以非零代码退出
- → **Cancelled**：用户运行 `gcancel <job_id>`
- → **Timeout**：任务超过时间限制（使用 `--time` 设置）

**从完成状态**：
- 无转换（最终状态）
- 使用 `gjob redo <job_id>` 创建具有相同参数的新任务

## 任务状态原因

某些状态的任务有关联的原因，提供更多上下文：

| 状态 | 原因 | 描述 |
|------|------|------|
| Queued | `WaitingForDependency` | 任务正在等待父任务完成 |
| Queued | `WaitingForResources` | 任务正在等待可用的 GPU/内存 |
| Hold | `JobHeldUser` | 任务被用户暂停 |
| Cancelled | `CancelledByUser` | 用户明确取消了任务 |
| Cancelled | `DependencyFailed:<job_id>` | 任务因任务 `<job_id>` 失败而自动取消 |
| Cancelled | `SystemError:<msg>` | 任务因系统错误而取消 |

使用 `gjob show <job_id>` 或 `gqueue -f JOBID,ST,REASON` 查看原因。

## 状态检查工作流

下图将流程收敛为“检查 -> 操作 -> 再检查”的循环：

```mermaid
---
showToolbar: true
---
flowchart TD
    Check([运行 gqueue -f JOBID,ST,REASON]) --> State{当前状态？}

    State -->|Queued| QueuedReason{原因？}
    QueuedReason -->|WaitingForDependency| Dep[检查父任务<br/>gqueue -t]
    QueuedReason -->|WaitingForResources| Res[检查资源<br/>ginfo]
    Dep --> Recheck([稍后再检查])
    Res --> Recheck

    State -->|Hold| Release[释放任务<br/>gjob release ID]
    Release --> Recheck

    State -->|Running| Monitor[查看日志或附着<br/>gjob log ID / gjob attach ID]
    Monitor --> Recheck

    State -->|Finished| Done([已完成])

    State -->|Failed| Retry[检查日志并在修复后重做<br/>gjob log ID / gjob redo ID]
    Retry --> Recheck

    State -->|Cancelled| CancelReason{原因？}
    CancelReason -->|CancelledByUser| Stop([无需进一步操作])
    CancelReason -->|DependencyFailed| Cascade[修复父任务并级联重做<br/>gjob redo PARENT_ID --cascade]
    Cascade --> Recheck

    State -->|Timeout| MoreTime[增加时间后重做<br/>gjob redo ID --time HH:MM:SS]
    MoreTime --> Recheck
```

## 另请参阅

- [任务依赖](./job-dependencies) - 任务依赖完整指南
- [任务提交](./job-submission) - 任务提交选项
- [时间限制](./time-limits) - 管理任务超时
