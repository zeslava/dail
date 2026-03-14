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
    Log {
        path: String,
    },
    Service {
        name: String,
        create_user: bool,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dailfile {
    pub instructions: Vec<Instruction>,
}

impl Dailfile {
    pub fn parse(input: &str) -> Result<Self, DailError> {
        tracing::info!("Starting Dailfile parse ({} bytes)", input.len());
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
                    let (src, dst) = rest.split_once(char::is_whitespace).ok_or_else(|| {
                        DailError::Build(format!(
                            "line {}: COPY requires <src> <dst>",
                            line_num + 1
                        ))
                    })?;
                    Instruction::Copy {
                        source: src.trim().to_string(),
                        destination: dst.trim().to_string(),
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
                    let readonly = rest.starts_with("ro:");
                    let spec = if readonly { &rest[3..] } else { rest };
                    let (src, dst) = spec.split_once(':').ok_or_else(|| {
                        DailError::Build(format!(
                            "line {}: MOUNT requires <src>:<dst>",
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
                "LOG" => Instruction::Log {
                    path: rest.to_string(),
                },
                "SERVICE" => {
                    let mut parts = rest.split_whitespace();
                    let svc_name = parts
                        .next()
                        .ok_or_else(|| {
                            DailError::Build(format!(
                                "line {}: SERVICE requires a name",
                                line_num + 1
                            ))
                        })?
                        .to_string();
                    let create_user = !parts.any(|p| p == "--no-user");
                    Instruction::Service {
                        name: svc_name,
                        create_user,
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

        tracing::info!("Dailfile parsed: {} instructions", instructions.len());
        Ok(Dailfile { instructions })
    }
}
