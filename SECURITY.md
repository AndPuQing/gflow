# Security Policy

Thank you for helping keep `gflow` and `runqd` safe.

`gflow` is a local job scheduler for shared Linux machines. Because it can start processes, manage job state, and expose optional local services, security reports are taken seriously.

## Supported Versions

Security fixes are provided on a best-effort basis for:

| Version | Supported |
| --- | --- |
| Latest published release | Yes |
| `main` branch | Yes |
| Older releases | No |

If you are unsure whether your installation is still supported, test against the latest release first.

## Reporting a Vulnerability

Please report vulnerabilities privately. Do not open a public GitHub issue, discussion, or pull request for suspected security problems.

Send a report to [me@puqing.work](mailto:me@puqing.work) with the subject line `gflow security report`.

Please include as much of the following as possible:

- A clear description of the issue and its impact.
- The affected `gflow` / `runqd` version.
- How you installed it, such as Cargo, `pip`, `pipx`, or `uv tool`.
- Your environment, especially Linux distribution, kernel, and whether GPU scheduling is enabled.
- Step-by-step reproduction instructions or a proof of concept.
- Any suggested mitigations if you already tested one.

If you want to share sensitive details but need a different channel first, mention that in the email and we can coordinate.

## Response Process

Best-effort process:

- Initial acknowledgement within 72 hours.
- Triage and severity assessment after reproduction.
- Status updates while a fix or mitigation is being prepared.
- Public disclosure after a fix is available, when appropriate.

## Deployment Notes

To reduce risk in production-like environments:

- Keep `gflow` updated to the latest release.
- Limit who can access the host account that runs `gflowd`.
- Expose network services only when necessary, and prefer loopback-only binding unless you understand the security implications.
- Treat submitted jobs, scripts, and environment variables as privileged inputs.
