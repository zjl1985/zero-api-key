use gpui::{Hsla, rgb};

#[derive(Clone, Copy)]
pub struct Palette {
    pub base: Hsla,
    pub sidebar: Hsla,
    pub panel: Hsla,
    pub hover: Hsla,
    pub border: Hsla,
    pub text: Hsla,
    pub muted: Hsla,
    pub accent: Hsla,
    pub selected: Hsla,
    pub danger: Hsla,
}
impl Palette {
    pub fn new(light: bool) -> Self {
        let values = if light {
            [
                0xfafaf8, 0xf1f2ee, 0xffffff, 0xe9ece6, 0xdfe3dc, 0x242d29, 0x7a837e, 0x40745b,
                0xe0eae2, 0xb34f4f,
            ]
        } else {
            [
                0x171b1b, 0x131717, 0x1d2222, 0x262d2b, 0x2b3330, 0xe2e7e1, 0x84908a, 0xa3c9b0,
                0x293b31, 0xde9790,
            ]
        };
        let [
            base,
            sidebar,
            panel,
            hover,
            border,
            text,
            muted,
            accent,
            selected,
            danger,
        ] = values.map(|v| rgb(v).into());
        Self {
            base,
            sidebar,
            panel,
            hover,
            border,
            text,
            muted,
            accent,
            selected,
            danger,
        }
    }
}
