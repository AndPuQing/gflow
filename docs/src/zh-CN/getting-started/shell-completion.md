# Shell 补全

Shell 补全为 gflow 命令、选项和参数提供自动补全功能，当您按下 Tab 键时即可使用。这大大改善了命令行体验，减少了输入并帮助您发现可用的选项。

## 概述

所有 gflow 命令都支持以下 shell 的补全功能：
- **Bash**
- **Zsh**
- **Fish**
- **PowerShell**
- **Elvish**

每个 gflow 命令（`gbatch`、`gqueue`、`ginfo`、`gcancel`、`gjob`、`gctl`、`gflowd`）都有一个内置的 `completion` 子命令，可以为您的 shell 生成相应的补全脚本。

## 快速设置

### Bash

将以下内容添加到您的 `~/.bashrc`：

```bash
# gflow 补全
eval "$(gbatch completion bash)"
eval "$(gqueue completion bash)"
eval "$(ginfo completion bash)"
eval "$(gcancel completion bash)"
eval "$(gjob completion bash)"
eval "$(gctl completion bash)"
eval "$(gflowd completion bash)"
```

然后重新加载您的 shell：
```bash
source ~/.bashrc
```

### Zsh

将以下内容添加到您的 `~/.zshrc`：

```zsh
# gflow 补全
eval "$(gbatch completion zsh)"
eval "$(gqueue completion zsh)"
eval "$(ginfo completion zsh)"
eval "$(gcancel completion zsh)"
eval "$(gjob completion zsh)"
eval "$(gctl completion zsh)"
eval "$(gflowd completion zsh)"
```

然后重新加载您的 shell：
```zsh
source ~/.zshrc
```

### Fish

将以下内容添加到您的 `~/.config/fish/config.fish`：

```fish
# gflow 补全
gbatch completion fish | source
gqueue completion fish | source
ginfo completion fish | source
gcancel completion fish | source
gjob completion fish | source
gctl completion fish | source
gflowd completion fish | source
```

然后重新加载您的 shell：
```fish
source ~/.config/fish/config.fish
```

### PowerShell

将以下内容添加到您的 PowerShell 配置文件（通常是 `$PROFILE`）：

```powershell
# gflow 补全
gbatch completion powershell | Out-String | Invoke-Expression
gqueue completion powershell | Out-String | Invoke-Expression
ginfo completion powershell | Out-String | Invoke-Expression
gcancel completion powershell | Out-String | Invoke-Expression
gjob completion powershell | Out-String | Invoke-Expression
gctl completion powershell | Out-String | Invoke-Expression
gflowd completion powershell | Out-String | Invoke-Expression
```

然后重新加载您的配置文件：
```powershell
. $PROFILE
```

### Elvish

将以下内容添加到您的 `~/.elvish/rc.elv`：

```elvish
# gflow 补全
eval (gbatch completion elvish | slurp)
eval (gqueue completion elvish | slurp)
eval (ginfo completion elvish | slurp)
eval (gcancel completion elvish | slurp)
eval (gjob completion elvish | slurp)
eval (gctl completion elvish | slurp)
eval (gflowd completion elvish | slurp)
```

## 替代设置：静态文件

如果您希望一次性生成补全文件并加载它们（这在 shell 启动时会更快），可以将补全保存到文件中。

### Bash

```bash
# 如果目录不存在则创建
mkdir -p ~/.local/share/bash-completion/completions

# 生成补全文件
gbatch completion bash > ~/.local/share/bash-completion/completions/gbatch
gqueue completion bash > ~/.local/share/bash-completion/completions/gqueue
ginfo completion bash > ~/.local/share/bash-completion/completions/ginfo
gcancel completion bash > ~/.local/share/bash-completion/completions/gcancel
gjob completion bash > ~/.local/share/bash-completion/completions/gjob
gctl completion bash > ~/.local/share/bash-completion/completions/gctl
gflowd completion bash > ~/.local/share/bash-completion/completions/gflowd
```

如果您安装了 bash-completion，Bash 会在启动时自动加载这些补全。

### Zsh

```bash
# 如果目录不存在则创建
mkdir -p ~/.zsh/completions

# 生成补全文件
gbatch completion zsh > ~/.zsh/completions/_gbatch
gqueue completion zsh > ~/.zsh/completions/_gqueue
ginfo completion zsh > ~/.zsh/completions/_ginfo
gcancel completion zsh > ~/.zsh/completions/_gcancel
gjob completion zsh > ~/.zsh/completions/_gjob
gctl completion zsh > ~/.zsh/completions/_gctl
gflowd completion zsh > ~/.zsh/completions/_gflowd
```

然后将以下内容添加到您的 `~/.zshrc`（在 `compinit` 之前）：

```zsh
fpath=(~/.zsh/completions $fpath)
autoload -Uz compinit && compinit
```

### Fish

```bash
# 如果目录不存在则创建
mkdir -p ~/.config/fish/completions

# 生成补全文件
gbatch completion fish > ~/.config/fish/completions/gbatch.fish
gqueue completion fish > ~/.config/fish/completions/gqueue.fish
ginfo completion fish > ~/.config/fish/completions/ginfo.fish
gcancel completion fish > ~/.config/fish/completions/gcancel.fish
gjob completion fish > ~/.config/fish/completions/gjob.fish
gctl completion fish > ~/.config/fish/completions/gctl.fish
gflowd completion fish > ~/.config/fish/completions/gflowd.fish
```

Fish 会自动加载这些补全。

## 下一步

现在您已经设置了 shell 补全，可以更高效地使用 gflow。前往[快速入门指南](./quick-start)了解如何使用 gflow 命令。

---

**上一页**：[安装](./installation) | **下一页**：[快速入门](./quick-start)
