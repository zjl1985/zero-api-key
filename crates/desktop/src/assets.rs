use gpui::{AssetSource, SharedString};
use std::borrow::Cow;

pub struct Assets;
impl AssetSource for Assets {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        let body = match path {
            "key" => r#"<circle cx="8" cy="8" r="4"/><path d="m11 11 9 9m-5-5 3-3m-1 5 3-3"/>"#,
            "folder" => r#"<path d="M3 7V5h6l2 2h10v12H3Z"/>"#,
            "search" => r#"<circle cx="10" cy="10" r="6"/><path d="m15 15 6 6"/>"#,
            "copy" => {
                r#"<rect x="8" y="8" width="12" height="13" rx="2"/><path d="M16 8V3H3v13h5"/>"#
            }
            "lock" => {
                r#"<rect x="5" y="10" width="14" height="11" rx="2"/><path d="M8 10V7a4 4 0 0 1 8 0v3m-4 4v3"/>"#
            }
            "settings" => {
                r#"<path d="M4 6h16M4 12h16M4 18h16"/><circle cx="9" cy="6" r="2"/><circle cx="15" cy="12" r="2"/><circle cx="9" cy="18" r="2"/>"#
            }
            "sync" => {
                r#"<path d="M20 9a8 8 0 0 0-14-3L3 9m0-6v6h6m-5 6a8 8 0 0 0 14 3l3-3m0 6v-6h-6"/>"#
            }
            "plus" => r#"<path d="M12 5v14M5 12h14"/>"#,
            "eye" => {
                r#"<path d="M2 12s4-7 10-7 10 7 10 7-4 7-10 7S2 12 2 12Z"/><circle cx="12" cy="12" r="3"/>"#
            }
            "edit" => r#"<path d="m15 4 5 5M4 20l1-5L16 4a3 3 0 0 1 4 4L9 19Z"/>"#,
            "trash" => r#"<path d="M3 6h18M9 6V3h6v3M5 6l1 15h12l1-15M10 10v7m4-7v7"/>"#,
            "import" => r#"<path d="M12 3v12m-5-5 5 5 5-5M4 16v5h16v-5"/>"#,
            "export" => r#"<path d="M12 15V3m-5 5 5-5 5 5M4 16v5h16v-5"/>"#,
            _ => return Ok(None),
        };
        Ok(Some(Cow::Owned(format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">{body}</svg>"#).into_bytes())))
    }
    fn list(&self, _: &str) -> anyhow::Result<Vec<SharedString>> {
        Ok(vec![])
    }
}
