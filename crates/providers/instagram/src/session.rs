//! The Instagram session credentials and their `Cookie` header rendering.

/// Instagram session cookies plus an optional device id used for request headers.
#[derive(Debug, Clone)]
pub struct InstagramSession {
    pub sessionid: String,
    pub ds_user_id: String,
    pub device_id: Option<String>,
    pub csrftoken: Option<String>,
}

impl InstagramSession {
    /// Renders the `Cookie` header value from the session parts.
    pub fn cookie_header(&self) -> String {
        let mut parts = vec![
            format!("sessionid={}", self.sessionid),
            format!("ds_user_id={}", self.ds_user_id),
        ];
        if let Some(csrftoken) = &self.csrftoken {
            parts.push(format!("csrftoken={csrftoken}"));
        }
        parts.join("; ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(csrftoken: Option<&str>) -> InstagramSession {
        InstagramSession {
            sessionid: "SID".to_owned(),
            ds_user_id: "4242".to_owned(),
            device_id: None,
            csrftoken: csrftoken.map(str::to_owned),
        }
    }

    #[test]
    fn cookie_header_joins_session_and_user_id() {
        assert_eq!(session(None).cookie_header(), "sessionid=SID; ds_user_id=4242");
    }

    #[test]
    fn cookie_header_includes_csrftoken_when_present() {
        assert_eq!(
            session(Some("TOKEN")).cookie_header(),
            "sessionid=SID; ds_user_id=4242; csrftoken=TOKEN"
        );
    }
}
