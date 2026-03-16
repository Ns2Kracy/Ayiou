use anyhow::{Result, anyhow, bail};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandAction {
    Sub { uid: u64 },
    Unsub { uid: u64 },
    List,
}

impl CommandAction {
    pub fn parse(input: &str) -> Result<Self> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            bail!("missing subcommand");
        }

        let mut parts = trimmed.split_whitespace();
        let action = parts.next().ok_or_else(|| anyhow!("missing subcommand"))?;

        match action {
            "sub" => {
                let uid = parse_uid(parts.next())?;
                if parts.next().is_some() {
                    bail!("unexpected extra arguments");
                }
                Ok(Self::Sub { uid })
            }
            "unsub" => {
                let uid = parse_uid(parts.next())?;
                if parts.next().is_some() {
                    bail!("unexpected extra arguments");
                }
                Ok(Self::Unsub { uid })
            }
            "list" => {
                if parts.next().is_some() {
                    bail!("list does not take arguments");
                }
                Ok(Self::List)
            }
            other => bail!("unknown subcommand: {}", other),
        }
    }
}

fn parse_uid(raw: Option<&str>) -> Result<u64> {
    let raw = raw.ok_or_else(|| anyhow!("missing uid"))?;
    raw.parse::<u64>()
        .map_err(|_| anyhow!("uid must be a number"))
}
