# ginfo Reference

`ginfo` shows scheduler and GPU allocation information.

If a GPU is occupied by a non-gflow compute process, it may be shown with a reason like `Unmanaged`, and gflow will not allocate it until it becomes idle.

## Usage

```bash
ginfo
ginfo completion <shell>
```

## Examples

```bash
ginfo
watch -n 2 ginfo
```

## Options

- `-v/-vv/-q`: adjust verbosity
- `--config <path>`: use a custom config file (hidden)
