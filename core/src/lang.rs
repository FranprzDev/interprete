#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    Es,
    En,
}

impl Lang {
    pub fn code(self) -> &'static str {
        match self {
            Lang::Es => "es",
            Lang::En => "en",
        }
    }
    pub fn english_name(self) -> &'static str {
        match self {
            Lang::Es => "Spanish",
            Lang::En => "English",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Lang::Es => "Español",
            Lang::En => "Inglés",
        }
    }
    pub fn other(self) -> Lang {
        match self {
            Lang::Es => Lang::En,
            Lang::En => Lang::Es,
        }
    }
    pub fn parse(s: &str) -> Option<Lang> {
        match s.to_lowercase().as_str() {
            "es" | "spa" | "spanish" | "español" => Some(Lang::Es),
            "en" | "eng" | "english" | "inglés" | "ingles" => Some(Lang::En),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parse_and_other() {
        assert_eq!(Lang::parse("ES"), Some(Lang::Es));
        assert_eq!(Lang::parse("english"), Some(Lang::En));
        assert_eq!(Lang::parse("fr"), None);
        assert_eq!(Lang::Es.other(), Lang::En);
    }
}
