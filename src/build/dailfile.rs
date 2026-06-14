use crate::error::DailError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Instruction {
    From {
        release: String,
    },
    Run {
        command: String,
    },
    Copy {
        source: String,
        destination: String,
        chown: Option<String>,
    },
    Env {
        key: String,
        value: String,
    },
    Param {
        key: String,
        value: String,
    },
    Mount {
        source: String,
        destination: String,
        readonly: bool,
    },
    Cmd {
        command: String,
    },
    Workdir {
        path: String,
    },
    Log {
        path: String,
    },
    Service {
        name: String,
        command: String,
        command_args: Vec<String>,
        create_user: bool,
        run_user: Option<String>,
    },
    Expose {
        host_port: u16,
        jail_port: u16,
        proto: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dailfile {
    pub instructions: Vec<Instruction>,
}

impl Dailfile {
    pub fn parse(input: &str) -> Result<Self, DailError> {
        tracing::info!("Parsing .dail file ({} bytes)", input.len());
        let mut instructions = Vec::new();

        for (line_num, line) in input.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            tracing::debug!("Parsing line {}: {}", line_num + 1, line);
            let (keyword, rest) = line.split_once(char::is_whitespace).ok_or_else(|| {
                DailError::Build(format!(
                    "line {}: invalid instruction: {line}",
                    line_num + 1
                ))
            })?;
            let rest = rest.trim();

            let instruction = match keyword.to_uppercase().as_str() {
                "FROM" => Instruction::From {
                    release: rest.to_string(),
                },
                "RUN" => Instruction::Run {
                    command: rest.to_string(),
                },
                "COPY" => {
                    let mut chown = None;
                    let rest = if rest.starts_with("--chown=") {
                        let (flag, remainder) = rest.split_once(char::is_whitespace).ok_or_else(|| {
                            DailError::Build(format!(
                                "line {}: COPY --chown requires <src> <dst>",
                                line_num + 1
                            ))
                        })?;
                        chown = Some(flag.strip_prefix("--chown=").unwrap().to_string());
                        remainder.trim()
                    } else {
                        rest
                    };
                    let (src, dst) = rest.split_once(char::is_whitespace).ok_or_else(|| {
                        DailError::Build(format!(
                            "line {}: COPY requires <src> <dst>",
                            line_num + 1
                        ))
                    })?;
                    Instruction::Copy {
                        source: src.trim().to_string(),
                        destination: dst.trim().to_string(),
                        chown,
                    }
                }
                "ENV" => {
                    let (key, value) = rest.split_once('=').ok_or_else(|| {
                        DailError::Build(format!("line {}: ENV requires KEY=VALUE", line_num + 1))
                    })?;
                    Instruction::Env {
                        key: key.trim().to_string(),
                        value: value.trim().to_string(),
                    }
                }
                "PARAM" => {
                    let (key, value) = rest.split_once('=').ok_or_else(|| {
                        DailError::Build(format!("line {}: PARAM requires KEY=VALUE", line_num + 1))
                    })?;
                    Instruction::Param {
                        key: key.trim().to_string(),
                        value: value.trim().to_string(),
                    }
                }
                "MOUNT" => {
                    let (spec, readonly) = if rest.ends_with(":ro") {
                        (&rest[..rest.len() - 3], true)
                    } else {
                        (rest, false)
                    };
                    let (src, dst) = spec.split_once(':').ok_or_else(|| {
                        DailError::Build(format!(
                            "line {}: MOUNT requires <src>:<dst>[:<ro>]",
                            line_num + 1
                        ))
                    })?;
                    Instruction::Mount {
                        source: src.to_string(),
                        destination: dst.to_string(),
                        readonly,
                    }
                }
                "CMD" => Instruction::Cmd {
                    command: rest.to_string(),
                },
                "WORKDIR" => Instruction::Workdir {
                    path: rest.to_string(),
                },
                "LOG" => Instruction::Log {
                    path: rest.to_string(),
                },
                "EXPOSE" => {
                    // Split off /proto suffix (default: tcp)
                    let (port_part, proto) = if let Some((p, pr)) = rest.rsplit_once('/') {
                        (p, pr.to_string())
                    } else {
                        (rest, "tcp".to_string())
                    };
                    let parts: Vec<&str> = port_part.split(':').collect();
                    match parts.len() {
                        1 => {
                            let port: u16 = parts[0].trim().parse().map_err(|_| {
                                DailError::Build(format!("line {}: invalid port: {}", line_num + 1, parts[0]))
                            })?;
                            Instruction::Expose { host_port: port, jail_port: port, proto }
                        }
                        2 => {
                            let host_port: u16 = parts[0].trim().parse().map_err(|_| {
                                DailError::Build(format!("line {}: invalid host port: {}", line_num + 1, parts[0]))
                            })?;
                            let jail_port: u16 = parts[1].trim().parse().map_err(|_| {
                                DailError::Build(format!("line {}: invalid jail port: {}", line_num + 1, parts[1]))
                            })?;
                            Instruction::Expose { host_port, jail_port, proto }
                        }
                        _ => return Err(DailError::Build(format!(
                            "line {}: EXPOSE requires port[/proto] or host_port:jail_port[/proto]", line_num + 1
                        ))),
                    }
                }
                "SERVICE" => {
                    let mut parts: Vec<&str> = rest.split_whitespace().collect();
                    let svc_name = parts
                        .first()
                        .ok_or_else(|| {
                            DailError::Build(format!(
                                "line {}: SERVICE requires a name",
                                line_num + 1
                            ))
                        })?
                        .to_string();
                    let create_user = !parts.iter().any(|p| *p == "--no-user");
                    let run_user = parts.iter()
                        .find(|p| p.starts_with("--run-user="))
                        .map(|p| p.trim_start_matches("--run-user=").to_string());
                    parts.retain(|p| !p.starts_with("--"));
                    let command = if parts.len() > 1 {
                        parts[1].to_string()
                    } else {
                        format!("/usr/local/bin/{svc_name}")
                    };
                    let command_args = if parts.len() > 2 {
                        parts[2..].iter().map(|s| s.to_string()).collect()
                    } else {
                        vec![]
                    };
                    Instruction::Service {
                        name: svc_name,
                        command,
                        command_args,
                        create_user,
                        run_user,
                    }
                }
                other => {
                    return Err(DailError::Build(format!(
                        "line {}: unknown instruction: {other}",
                        line_num + 1
                    )));
                }
            };

            instructions.push(instruction);
        }

        tracing::info!("Parsed: {} instructions", instructions.len());
        Ok(Dailfile { instructions })
    }
}
