use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize, PartialEq, Eq, Clone, Copy)]
pub struct Colors {
    #[serde(deserialize_with = "hex_color")]
    pub bg: u32,
    #[serde(deserialize_with = "hex_color")]
    pub text: u32,
    #[serde(deserialize_with = "hex_color")]
    pub line: u32,
}

#[derive(Debug, Deserialize, PartialEq, Eq, Clone, Copy)]
pub struct Config {
    pub colors: Colors,
}

fn hex_color<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s: String = serde::Deserialize::deserialize(deserializer)?;
    let s = s.trim_start_matches("0x").trim_start_matches("0X");
    u32::from_str_radix(s, 16).map_err(serde::de::Error::custom)
}

impl Config {
    pub fn from_str(s: &str) -> Result<Config, String> {
        toml::from_str(s).map_err(|e| e.to_string())
    }

    pub fn load_from_path(p: &Path) -> Result<Config, String> {
        let text = std::fs::read_to_string(p).map_err(|e| e.to_string())?;
        Config::from_str(&text)
    }

    pub fn default_colors() -> Config {
        Config {
            colors: Colors {
                bg: 0x1E1E1E,
                text: 0xD4D4D4,
                line: 0x3C3C3C,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_toml() {
        let toml_str = r#"
            [colors]
            bg = "0x1E1E1E"
            text = "0xD4D4D4"
            line = "0x3C3C3C"
        "#;
        let cfg = Config::from_str(toml_str).expect("should parse");
        assert_eq!(cfg.colors.bg, 0x1E1E1E);
        assert_eq!(cfg.colors.text, 0xD4D4D4);
        assert_eq!(cfg.colors.line, 0x3C3C3C);
    }

    #[test]
    fn rejects_malformed_toml() {
        let result = Config::from_str("not valid toml {{{");
        assert!(result.is_err());
    }

    #[test]
    fn default_colors_match_current_ahk_prototype() {
        let cfg = Config::default_colors();
        assert_eq!(cfg.colors.bg, 0x1E1E1E);
        assert_eq!(cfg.colors.text, 0xD4D4D4);
        assert_eq!(cfg.colors.line, 0x3C3C3C);
    }
}
