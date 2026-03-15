use crate::build::dailfile::{Dailfile, Instruction};
use crate::error::DailError;
use crate::freebsd::mount::NullfsMount;
use crate::jail::config::{JailConfig, MountSpec, PortMapping};
use crate::jail::lifecycle::JailLifecycle;

pub struct BuildExecutor;

impl BuildExecutor {
    pub fn build(
        lifecycle: &mut JailLifecycle,
        dailfile: &Dailfile,
        name: &str,
        cli_config: Option<&JailConfig>,
        context_dir: &std::path::Path,
    ) -> Result<(), DailError> {
        tracing::info!("Building jail '{}'", name);
        let mut jail_config = JailConfig::new(name.to_string());
        // Build always uses thick jail — we need a writable rootfs
        jail_config.jail_type = crate::jail::config::JailType::Thick;

        let has_cli_ports = cli_config.map_or(false, |c| !c.ports.is_empty());

        tracing::debug!("Processing instructions...");
        for instruction in &dailfile.instructions {
            match instruction {
                Instruction::From { release } => {
                    jail_config.base = Some(release.clone());
                }
                Instruction::Param { key, value } => {
                    if key == "persist" {
                        jail_config.persist = value == "true";
                    } else {
                        jail_config.params.insert(key.clone(), value.clone());
                    }
                }
                Instruction::Mount {
                    source,
                    destination,
                    readonly,
                } => {
                    jail_config.mounts.push(MountSpec {
                        source: source.into(),
                        destination: destination.into(),
                        readonly: *readonly,
                        fs_type: "nullfs".to_string(),
                    });
                }
                Instruction::Cmd { command } => {
                    jail_config.cmd = Some(command.clone());
                }
                Instruction::Log { path } => {
                    jail_config.log_file = Some(path.clone());
                }
                Instruction::Expose { host_port, jail_port } => {
                    // Skip EXPOSE if -p was provided on CLI
                    if !has_cli_ports {
                        jail_config.ports.push(PortMapping {
                            host_ip: None,
                            host_port: *host_port,
                            jail_port: *jail_port,
                            proto: "tcp".to_string(),
                        });
                    }
                }
                _ => {}
            }
        }

        // SERVICE sets default CMD if no explicit CMD was given
        let has_explicit_cmd = dailfile
            .instructions
            .iter()
            .any(|i| matches!(i, Instruction::Cmd { .. }));
        if !has_explicit_cmd {
            for instruction in &dailfile.instructions {
                if let Instruction::Service { name, .. } = instruction {
                    jail_config.cmd = Some(format!("service {name} start"));
                    break;
                }
            }
        }

        // CLI args override .dail file (network, hostname, mounts from CLI take precedence)
        if let Some(cli) = cli_config {
            jail_config.network = cli.network.clone();
            if cli.hostname.is_some() {
                jail_config.hostname = cli.hostname.clone();
            }
            jail_config.mounts.extend(cli.mounts.iter().cloned());
            jail_config.auto_remove = cli.auto_remove;
            jail_config.ports.extend(cli.ports.iter().cloned());
        }

        let jail_config_final = jail_config.clone();
        tracing::info!(
            "Jail config prepared: base={:?}, type=thick, persist={}",
            jail_config.base,
            jail_config.persist
        );
        tracing::info!("Creating jail '{}'", name);
        lifecycle.create(jail_config)?;
        tracing::info!("Jail '{}' created", name);

        let has_run_or_copy = dailfile
            .instructions
            .iter()
            .any(|i| matches!(i, Instruction::Run { .. } | Instruction::Copy { .. }));
        if has_run_or_copy {
            // During build: persist=true to stay alive between RUN steps,
            // inherit network for pkg access, no CMD yet
            lifecycle.store_update(name, |s| {
                s.config.persist = true;
                s.config.network = crate::network::NetworkConfig::Inherit;
                s.config.cmd = None;
            })?;
            tracing::info!("Starting jail '{}' for build", name);
            match lifecycle.start(name) {
                Ok(_) => {
                    tracing::info!("Jail '{}' started", name);
                }
                Err(e) => {
                    tracing::error!("Failed to start jail '{}' for build: {}", name, e);
                    return Err(e);
                }
            }

            // Copy resolv.conf from host so pkg can resolve DNS
            let root_path = lifecycle
                .get(name)
                .ok_or_else(|| DailError::Build("jail not found".into()))?
                .root_path
                .clone();
            let host_resolv = std::path::Path::new("/etc/resolv.conf");
            if host_resolv.exists() {
                let _ = std::fs::copy(host_resolv, root_path.join("etc/resolv.conf"));
            }

            // Copy pkg-static from host to skip bootstrap
            let host_pkg = std::path::Path::new("/usr/local/sbin/pkg-static");
            if host_pkg.exists() {
                let jail_pkg_dir = root_path.join("usr/local/sbin");
                std::fs::create_dir_all(&jail_pkg_dir)?;
                std::fs::copy(host_pkg, jail_pkg_dir.join("pkg-static"))?;
                // pkg(8) bootstraps by checking for /usr/local/sbin/pkg
                let _ =
                    std::fs::hard_link(jail_pkg_dir.join("pkg-static"), jail_pkg_dir.join("pkg"));
            }

            // Mount shared pkg cache (downloaded packages + repo metadata)
            tracing::info!("Mounting shared pkg cache");
            let pkg_cache_host = lifecycle.global_config().pkg_cache_dir();
            std::fs::create_dir_all(&pkg_cache_host)?;
            let pkg_cache_jail = root_path.join("var/cache/pkg");
            std::fs::create_dir_all(&pkg_cache_jail)?;
            NullfsMount::mount(&pkg_cache_host, &pkg_cache_jail, false)?;

            let pkg_repos_host = lifecycle.global_config().pkg_repos_dir();
            std::fs::create_dir_all(&pkg_repos_host)?;
            let pkg_repos_jail = root_path.join("var/db/pkg/repos");
            std::fs::create_dir_all(&pkg_repos_jail)?;
            NullfsMount::mount(&pkg_repos_host, &pkg_repos_jail, false)?;

            tracing::info!("Starting build instructions execution");
            let exec_result = (|| -> Result<(), DailError> {
                for instruction in &dailfile.instructions {
                    match instruction {
                        Instruction::Run { command } => {
                            tracing::info!("RUN {}", command);
                            let status = lifecycle.exec(name, &["/bin/sh", "-c", command])?;
                            if !status.success() {
                                return Err(DailError::Build(format!(
                                    "RUN command failed with exit code {:?}: {command}",
                                    status.code()
                                )));
                            }
                        }
                        Instruction::Copy {
                            source,
                            destination,
                        } => {
                            tracing::info!("COPY {} -> {}", source, destination);
                            let jail_state = lifecycle
                                .get(name)
                                .ok_or_else(|| DailError::Build("jail not found".into()))?;
                            let dst = jail_state.root_path.join(
                                destination
                                    .strip_prefix("/")
                                    .unwrap_or(destination.as_str()),
                            );
                            if let Some(parent) = dst.parent() {
                                std::fs::create_dir_all(parent)?;
                            }
                            let src_raw = std::path::Path::new(source.as_str());
                            let src = if src_raw.is_relative() {
                                context_dir.join(src_raw)
                            } else {
                                src_raw.to_path_buf()
                            };
                            let src = src.as_path();
                            if src.is_dir() {
                                let status = std::process::Command::new("cp")
                                    .arg("-a")
                                    .arg(src)
                                    .arg(&dst)
                                    .status()
                                    .map_err(|e| DailError::Build(format!("cp failed: {e}")))?;
                                if !status.success() {
                                    return Err(DailError::Build("COPY directory failed".into()));
                                }
                            } else {
                                std::fs::copy(src, &dst)?;
                            }
                        }
                        Instruction::Env { key, value } => {
                            tracing::info!("ENV {}={}", key, value);
                            let profile = root_path.join("etc/profile");
                            use std::io::Write;
                            let mut f = std::fs::OpenOptions::new()
                                .create(true)
                                .append(true)
                                .open(&profile)
                                .map_err(|e| {
                                    DailError::Build(format!("failed to write /etc/profile: {e}"))
                                })?;
                            writeln!(f, "export {key}=\"{value}\"").map_err(|e| {
                                DailError::Build(format!("failed to write /etc/profile: {e}"))
                            })?;
                        }
                        Instruction::Service {
                            name: svc,
                            create_user,
                        } => {
                            tracing::info!("SERVICE {}", svc);

                            if *create_user {
                                tracing::info!("SERVICE {}: creating user/group/dirs", svc);
                                let setup_script = format!(
                                    "pw groupshow {svc} >/dev/null 2>&1 || pw groupadd -n {svc} && \
                                     pw usershow {svc} >/dev/null 2>&1 || pw useradd {svc} -d /var/lib/{svc} -g {svc} -m -s /usr/sbin/nologin && \
                                     mkdir -p /var/log/{svc} /var/run/{svc} /var/lib/{svc} && \
                                     chown {svc}:{svc} /var/log/{svc} /var/run/{svc} /var/lib/{svc}"
                                );
                                let status = lifecycle.exec(
                                    name,
                                    &["/bin/sh", "-c", &setup_script],
                                )?;
                                if !status.success() {
                                    return Err(DailError::Build(format!(
                                        "SERVICE {svc}: failed to create user/group/dirs (exit code {:?})",
                                        status.code()
                                    )));
                                }
                            }

                            let rc_conf = root_path.join("etc/rc.conf");
                            use std::io::Write;
                            let mut f = std::fs::OpenOptions::new()
                                .create(true)
                                .append(true)
                                .open(&rc_conf)
                                .map_err(|e| {
                                    DailError::Build(format!(
                                        "SERVICE {svc}: failed to write rc.conf: {e}"
                                    ))
                                })?;
                            writeln!(f, "{svc}_enable=\"YES\"").map_err(|e| {
                                DailError::Build(format!(
                                    "SERVICE {svc}: failed to write rc.conf: {e}"
                                ))
                            })?;
                        }
                        _ => {}
                    }
                }
                Ok(())
            })();

            // Always clean up pkg mounts and stop jail, regardless of success/failure
            let _ = NullfsMount::unmount(&pkg_repos_jail);
            let _ = NullfsMount::unmount(&pkg_cache_jail);
            lifecycle.stop(name)?;

            // Propagate any build error after cleanup
            exec_result?;

            // Restore final config: original params, cmd, network
            tracing::info!("Build completed for jail '{}'", name);
            lifecycle.store_update(name, |s| {
                s.config.persist = jail_config_final.persist;
                s.config.network = jail_config_final.network.clone();
                s.config.cmd = jail_config_final.cmd.clone();
                s.config.log_file = jail_config_final.log_file.clone();
                s.config.ports = jail_config_final.ports.clone();
            })?;
        }

        Ok(())
    }
}
