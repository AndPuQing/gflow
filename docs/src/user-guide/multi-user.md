# Multi-User Usage

gflow's multi-user model is a single machine shared by multiple Unix users, with one shared `gflowd` and one operating-system account per human user.

`gbatch` records the submitting username from the current shell environment (`USER` or `USERNAME`). By default, `gqueue` and `gstats` show the current user's jobs, so day-to-day usage already feels per-user even when the scheduler is shared.

::: warning Security Model
`gflowd` exposes an HTTP API and does not currently provide authentication or RBAC. If someone can reach the daemon endpoint, they can query scheduler state and send mutating requests.

For multi-user deployments, keep the daemon on `localhost` when possible, or restrict access with SSH, firewall rules, a VPN, or an authenticated proxy.
:::

## Recommended Setup

1. Run one shared `gflowd` on the machine.
2. Give each person a separate Unix account. Do not share one login.
3. Point every CLI to the same local daemon host and port.
4. Treat `gflowd` and `gctl` operational commands as administrator-only procedures.
5. Use projects and reservations to coordinate teams.

## For Administrators

### Run One Shared Daemon

Run a single scheduler for the machine instead of one daemon per user.

```toml
[daemon]
host = "localhost"
port = 59000
```

- `localhost` is the safest default for shared machines.
- Use a non-localhost bind address only when you have already planned the network boundary.
- State and logs are stored under the home directory of the account running `gflowd`; see [Configuration](./configuration#files-and-state).

### Standardize One Local Endpoint

Every user should point their CLI to the same local daemon. The default configuration is already `localhost:59000`, so no extra setup is usually needed.

### Enforce Admin/User Boundaries Outside gflow

Because gflow has no built-in roles, treat administrator privileges as an operating-system and network concern:

- Only trusted users should be able to reach the daemon endpoint.
- Only administrators should manage the daemon lifecycle (`gflowd up`, `gflowd down`, `gflowd restart`).
- Only administrators should use runtime control commands such as `gctl set-gpus` and team-wide reservation workflows.
- Avoid exposing `gflowd` directly on `0.0.0.0` unless you also add external access control.

### Standardize Team Metadata

If multiple teams share one scheduler, require project labels:

```toml
[projects]
known_projects = ["ml-research", "cv-team"]
require_project = true
```

Then users submit with:

```bash
gbatch --project ml-research python train.py
```

This makes `gqueue --project ...`, `gstats`, and notifications more useful.

### Coordinate Scarce GPUs

Use runtime restrictions and reservations when you need to protect capacity:

```bash
gctl set-gpus 0-3
gctl reserve create --user alice --gpus 2 --start '2026-01-28 14:00' --duration 2h
gctl reserve list --active
```

Reservations are especially useful for demos, deadlines, or shared lab time.

### Observe All Users

Useful administrator views:

```bash
gqueue -u all
gstats --user alice
gctl reserve list --timeline --range 48h
```

If you need external monitoring or audit hooks, add [Notifications](./notifications).

## For Regular Users

### Use the Default Local Connection

### Submit Jobs as Yourself

- Your job owner is taken from the current shell environment.
- Do not manually override `USER` or `USERNAME` when using the shared scheduler.
- Use a separate Unix account for each person instead of a shared account.

### Use the Default Per-User Views

The main overview commands already default to the current user:

```bash
gqueue
gstats
```

Use `gqueue` first when you need to find your own job IDs.

When your team uses project labels, add them consistently:

```bash
gbatch --project ml-research python train.py
gqueue --project ml-research
```

### Respect Shared Capacity Rules

- If administrators configured required projects, always pass `--project`.
- If GPUs are reserved for another user or restricted with `gctl set-gpus`, your job may remain queued until capacity is available.
- Ask an administrator before using reservation commands for team-wide scheduling changes.

### Know When to Contact an Administrator

Contact an administrator when:

- you cannot connect to the shared daemon;
- you need an exclusive GPU window for a demo or deadline;
- jobs are blocked by reservations or runtime GPU restrictions;
- the team wants to change shared project or notification policy.

## Common Patterns

### Shared Workstation

- All users log in to the same machine.
- `gflowd` listens on `localhost`.
- Each user runs `gbatch`, `gqueue`, and `gjob` from their own Unix account.

## See Also

- [Configuration](./configuration)
- [Job Submission](./job-submission)
- [Tips](./tips)
- [gctl Reference](../reference/gctl-reference)
- [gqueue Reference](../reference/gqueue-reference)
