# Security Policy

RouterUI runs as root on a machine that routes and firewalls a network, so
security reports are taken seriously.

## Reporting a vulnerability

Please **do not open a public issue** for a security problem.

Report it privately through GitHub: open the
[Security tab](https://github.com/bughatti/routerui/security) of this
repository and choose **Report a vulnerability**. Only the maintainer can see
the report.

Include what you found, the version or commit you tested, and steps to
reproduce it. You should get a first response within a week.

## Supported versions

Only the latest release receives security fixes.

## Scope

In scope: the backend API, the web interface, the installer (`install.sh`),
and the release binaries published on this repository. Examples: reaching the
API or web interface without authentication, command injection through
settings that end up in `iptables`, `ip` or other system commands, and
anything that lets a device on the LAN or WAN change router configuration.

Out of scope: vulnerabilities in the Linux kernel, iptables, dnsmasq, or other
system software RouterUI configures, unless RouterUI's use of them makes the
problem exploitable. Report those upstream.
