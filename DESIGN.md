# Dail Design Principles

This document defines the architectural philosophy and scope boundaries of **Dail**.
Its purpose is to keep the project simple, maintainable, and aligned with the FreeBSD ecosystem.

Dail is intentionally **not** a container platform.
It is a **simple tool for building and running FreeBSD jails for services and development workflows**.

---

# 1. Do One Thing Well

Dail focuses on a single responsibility:

**Build and run services inside FreeBSD jails.**

It does not attempt to solve:

* orchestration
* distributed systems
* container infrastructure
* cluster management

If a feature does not directly improve the workflow of creating or running a jail, it likely does not belong in Dail.

---

# 2. Prefer FreeBSD Primitives

Dail should leverage existing operating system functionality instead of re-implementing it.

Core primitives:

* jail(8)
* jexec(8)
* jls(8)
* pkg
* ZFS
* rc.d services
* nullfs / devfs
* rctl

Dail acts as a **workflow layer** on top of these tools, not a replacement for them.

---

# 3. Keep the System Small

Dail should remain a **small CLI tool**.

Guidelines:

* Avoid large subsystems.
* Avoid background services or daemons.
* Avoid complex internal frameworks.

The codebase should stay understandable by a single developer.

---

# 4. Git-Native Distribution

Dail recipes are distributed via **git repositories**.

Example workflow:

```
dail apply github.com/user/valkey
```

Repositories contain:

```
postgres.dail
Dail.lock
README.md
```

Dail does not require a central registry or server infrastructure.

---

# 5. One Recipe = One Service

A Dail recipe describes **one service**.

Examples:

* postgres
* valkey
* gitea
* nginx

Dail intentionally avoids multi-service orchestration.

If users want to run multiple services, they can run multiple jails.

---

# 6. .dail Files Must Stay Simple

The `.dail` file format is intentionally minimal.

Rules:

* Line-oriented syntax
* Small fixed set of instructions
* No scripting language
* No conditionals
* No templating
* No dependency graphs

If complex logic is required, it should be implemented with shell scripts instead.

---

# 7. Avoid Feature Creep

Certain feature classes are explicitly out of scope.

Dail will **not implement**:

* container registries
* orchestration systems
* docker-compose equivalents
* cluster management
* plugin systems
* dependency graphs
* health check schedulers
* restart policies
* remote execution across hosts

These systems introduce large complexity that conflicts with Dail's goals.

---

# 8. Prefer Transparency Over Abstraction

Users should understand what Dail is doing.

The system should expose and use standard FreeBSD mechanisms rather than hiding them behind heavy abstraction.

For example:

* jails remain visible through `jls`
* filesystems remain visible through `zfs`
* processes remain visible through `ps`

---

# 9. CLI First

Dail is primarily a **command-line tool**.

The CLI should remain:

* predictable
* scriptable
* stable

Interactive features are acceptable, but the primary interface should remain simple commands.

---

# 10. Simplicity Over Completeness

When choosing between:

* a simple solution that covers 80% of cases
* a complex solution that covers 100%

Dail should prefer the simpler option.

The goal is not to support every possible workflow, but to support common workflows extremely well.

---

# Summary

Dail aims to provide:

* simple jail workflows
* reproducible service environments
* git-native distribution
* minimal operational complexity

It should remain **small, predictable, and deeply integrated with FreeBSD** rather than evolving into a general container platform.
