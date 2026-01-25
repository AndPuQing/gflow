# gcancel Reference

`gcancel` cancels queued/running jobs.

## Usage

```bash
gcancel [--dry-run] <job_ids>
gcancel completion <shell>
```

`<job_ids>` supports:

- Single: `42`
- Comma-separated: `1,2,3`
- Range: `1-5`
- Mixed: `1,3,5-7,10`

## Examples

```bash
gcancel 42
gcancel 1,2,3
gcancel 10-20
```

### Dry Run

Preview which jobs can be cancelled, and queued/held jobs that depend on them:

```bash
gcancel --dry-run 42
```

