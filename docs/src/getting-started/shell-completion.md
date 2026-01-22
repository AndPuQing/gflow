# Shell Completion

Shell completion provides auto-completion for gflow commands, options, and arguments when you press the Tab key. This significantly improves the command-line experience by reducing typing and helping you discover available options.

## Overview

All gflow commands support shell completion for the following shells:
- **Bash**
- **Zsh**
- **Fish**
- **PowerShell**
- **Elvish**

Each gflow command (`gbatch`, `gqueue`, `ginfo`, `gcancel`, `gjob`, `gctl`, `gflowd`) has a built-in `completion` subcommand that generates the appropriate completion script for your shell.

## Quick Setup

### Bash

Add the following to your `~/.bashrc`:

```bash
# gflow completions
eval "$(gbatch completion bash)"
eval "$(gqueue completion bash)"
eval "$(ginfo completion bash)"
eval "$(gcancel completion bash)"
eval "$(gjob completion bash)"
eval "$(gctl completion bash)"
eval "$(gflowd completion bash)"
```

Then reload your shell:
```bash
source ~/.bashrc
```

### Zsh

Add the following to your `~/.zshrc`:

```zsh
# gflow completions
eval "$(gbatch completion zsh)"
eval "$(gqueue completion zsh)"
eval "$(ginfo completion zsh)"
eval "$(gcancel completion zsh)"
eval "$(gjob completion zsh)"
eval "$(gctl completion zsh)"
eval "$(gflowd completion zsh)"
```

Then reload your shell:
```zsh
source ~/.zshrc
```

### Fish

Add the following to your `~/.config/fish/config.fish`:

```fish
# gflow completions
gbatch completion fish | source
gqueue completion fish | source
ginfo completion fish | source
gcancel completion fish | source
gjob completion fish | source
gctl completion fish | source
gflowd completion fish | source
```

Then reload your shell:
```fish
source ~/.config/fish/config.fish
```

### PowerShell

Add the following to your PowerShell profile (usually `$PROFILE`):

```powershell
# gflow completions
gbatch completion powershell | Out-String | Invoke-Expression
gqueue completion powershell | Out-String | Invoke-Expression
ginfo completion powershell | Out-String | Invoke-Expression
gcancel completion powershell | Out-String | Invoke-Expression
gjob completion powershell | Out-String | Invoke-Expression
gctl completion powershell | Out-String | Invoke-Expression
gflowd completion powershell | Out-String | Invoke-Expression
```

Then reload your profile:
```powershell
. $PROFILE
```

### Elvish

Add the following to your `~/.elvish/rc.elv`:

```elvish
# gflow completions
eval (gbatch completion elvish | slurp)
eval (gqueue completion elvish | slurp)
eval (ginfo completion elvish | slurp)
eval (gcancel completion elvish | slurp)
eval (gjob completion elvish | slurp)
eval (gctl completion elvish | slurp)
eval (gflowd completion elvish | slurp)
```

## Alternative Setup: Static Files

If you prefer to generate completion files once and source them (which can be faster on shell startup), you can save the completions to files.

### Bash

```bash
# Create completion directory if it doesn't exist
mkdir -p ~/.local/share/bash-completion/completions

# Generate completion files
gbatch completion bash > ~/.local/share/bash-completion/completions/gbatch
gqueue completion bash > ~/.local/share/bash-completion/completions/gqueue
ginfo completion bash > ~/.local/share/bash-completion/completions/ginfo
gcancel completion bash > ~/.local/share/bash-completion/completions/gcancel
gjob completion bash > ~/.local/share/bash-completion/completions/gjob
gctl completion bash > ~/.local/share/bash-completion/completions/gctl
gflowd completion bash > ~/.local/share/bash-completion/completions/gflowd
```

Bash will automatically load these completions on startup if you have bash-completion installed.

### Zsh

```bash
# Create completion directory if it doesn't exist
mkdir -p ~/.zsh/completions

# Generate completion files
gbatch completion zsh > ~/.zsh/completions/_gbatch
gqueue completion zsh > ~/.zsh/completions/_gqueue
ginfo completion zsh > ~/.zsh/completions/_ginfo
gcancel completion zsh > ~/.zsh/completions/_gcancel
gjob completion zsh > ~/.zsh/completions/_gjob
gctl completion zsh > ~/.zsh/completions/_gctl
gflowd completion zsh > ~/.zsh/completions/_gflowd
```

Then add this to your `~/.zshrc` (before `compinit`):

```zsh
fpath=(~/.zsh/completions $fpath)
autoload -Uz compinit && compinit
```

### Fish

```bash
# Create completion directory if it doesn't exist
mkdir -p ~/.config/fish/completions

# Generate completion files
gbatch completion fish > ~/.config/fish/completions/gbatch.fish
gqueue completion fish > ~/.config/fish/completions/gqueue.fish
ginfo completion fish > ~/.config/fish/completions/ginfo.fish
gcancel completion fish > ~/.config/fish/completions/gcancel.fish
gjob completion fish > ~/.config/fish/completions/gjob.fish
gctl completion fish > ~/.config/fish/completions/gctl.fish
gflowd completion fish > ~/.config/fish/completions/gflowd.fish
```

Fish will automatically load these completions.

## Next Steps

Now that you have shell completion set up, you can work more efficiently with gflow. Head to the [Quick Start Guide](./quick-start) to learn how to use gflow commands.

---

**Previous**: [Installation](./installation) | **Next**: [Quick Start](./quick-start)
