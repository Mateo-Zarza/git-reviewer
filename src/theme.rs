use iced::{
    Border, Color, Theme,
    theme::Palette,
    widget::{button, container, rule, scrollable, text_editor},
};

pub mod colors {
    use iced::{Color, color};

    pub const BACKGROUND: Color = color!(0x18191c);
    pub const SURFACE: Color = color!(0x1f2023);
    pub const SIDEBAR: Color = color!(0x1c1d20);
    pub const BORDER: Color = color!(0x2e2f34);
    pub const CONTROL_BORDER: Color = color!(0x36373d);
    pub const TAG_BORDER: Color = color!(0x44464d);
    pub const HOVER: Color = color!(0x26272b);
    pub const SELECTED: Color = color!(0x2a3550);
    pub const FOCUS: Color = color!(0x5b9bff);
    pub const TEXT: Color = color!(0xe6e6e9);
    pub const TEXT_MUTED: Color = color!(0xa1a3ab);
    pub const TEXT_DIM: Color = color!(0x8b8e97);
    pub const LINE_NUMBER: Color = color!(0x6f727b);
    pub const PILL: Color = color!(0x3a3b41);
    pub const SCROLLBAR_HOVER: Color = color!(0x4a4c53);
    pub const PRIMARY: Color = color!(0x3d7be0);
    pub const ON_PRIMARY: Color = color!(0xffffff);
    pub const DANGER: Color = color!(0xf0857a);
    pub const DANGER_BORDER: Color = color!(0x5a3a37);
    pub const MODIFIED: Color = color!(0xe5b567);
    pub const ADDED: Color = color!(0x7cc48a);
    pub const RENAMED: Color = color!(0x6fb3e8);
    pub const DELETED: Color = DANGER;
    pub const ADDED_TINT: Color = color!(0x7cc48a, 0.13);
    pub const REMOVED_TINT: Color = color!(0xf0857a, 0.13);
    pub const ADDED_GUTTER: Color = color!(0x252f2a);
    pub const REMOVED_GUTTER: Color = color!(0x342728);
    pub const HUNK_BG: Color = color!(0x22252c);
    pub const HUNK_TEXT: Color = color!(0x8fa6c8);
}

pub mod text_sizes {
    pub const BODY: f32 = 13.0;
    pub const TITLE: f32 = 14.0;
    pub const BRANCH: f32 = 15.0;
    pub const SMALL: f32 = 12.0;
    pub const LABEL: f32 = 11.0;
    pub const TAG: f32 = 10.0;
}

pub mod layouts {
    // Bars and panels
    pub const TOP_BAR_HEIGHT: f32 = 44.0;
    pub const SIDEBAR_WIDTH: f32 = 260.0;
    pub const CHANGES_WIDTH: f32 = 280.0;
    // Buttons
    pub const BUTTON_HEIGHT: f32 = 30.0;
    pub const PRIMARY_BUTTON_HEIGHT: f32 = 32.0;
    pub const ICON_BUTTON: f32 = 22.0;
    // Diff
    pub const LINE_NUMBER_WIDTH: f32 = 44.0;
    pub const DIFF_LINE_HEIGHT: f32 = 20.0;
    pub const DIFF_SIGN_WIDTH: f32 = 16.0;
    /// Advance width of one JetBrains Mono glyph (0.6em) at the diff's 12px. Every glyph
    /// in a monospace font has it, so a line's width is `chars × this`.
    pub const DIFF_CHAR_WIDTH: f32 = 7.2;
    pub const DIFF_HEADER_HEIGHT: f32 = 44.0;
    // Padding
    pub const PANEL_PADDING_X: f32 = 16.0;
    pub const PANEL_PADDING_Y: f32 = 12.0;
    pub const ROW_PADDING_X: f32 = 8.0;
    pub const ROW_PADDING_Y: f32 = 5.0;
    // Gaps
    pub const GAP: f32 = 8.0;
    pub const SECTION_GAP: f32 = 10.0;
    pub const REPO_GAP: f32 = 14.0;
}

pub mod radii {
    pub const CONTROL: f32 = 6.0;
    pub const TAG: f32 = 4.0;
    pub const PILL: f32 = 9.0;
}

pub mod fonts {
    use iced::{Font, font::Weight};

    pub const UI: Font = Font::with_name("Inter");
    pub const UI_MEDIUM: Font = Font {
        weight: Weight::Medium,
        ..UI
    };
    pub const UI_SEMIBOLD: Font = Font {
        weight: Weight::Semibold,
        ..UI
    };
    pub const MONO: Font = Font::with_name("JetBrains Mono");
    pub const MONO_MEDIUM: Font = Font {
        weight: Weight::Medium,
        ..MONO
    };
}

pub mod icons {
    use iced::{
        Color,
        widget::{Svg, svg},
    };

    pub const DOWNLOAD: &[u8] = include_bytes!("../assets/icons/arrow-down-to-line.svg");
    pub const CHEVRON_DOWN: &[u8] = include_bytes!("../assets/icons/chevron-down.svg");
    pub const GIT_BRANCH: &[u8] = include_bytes!("../assets/icons/git-branch.svg");
    pub const MINUS: &[u8] = include_bytes!("../assets/icons/minus.svg");
    pub const PLUS: &[u8] = include_bytes!("../assets/icons/plus.svg");
    pub const ROTATE_CW: &[u8] = include_bytes!("../assets/icons/rotate-cw.svg");

    pub fn icon<'a>(bytes: &'static [u8], size: f32, color: Color) -> Svg<'a> {
        svg(svg::Handle::from_memory(bytes))
            .width(size)
            .height(size)
            .style(move |_, _| svg::Style { color: Some(color) })
    }
}

pub fn divider(theme: &Theme) -> rule::Style {
    rule::Style {
        color: colors::BORDER,
        ..rule::default(theme)
    }
}

pub fn outline_button(_theme: &Theme, status: button::Status) -> button::Style {
    let base = button::Style {
        background: None,
        text_color: colors::TEXT,
        border: Border {
            color: colors::CONTROL_BORDER,
            width: 1.0,
            radius: radii::CONTROL.into(),
        },
        ..button::Style::default()
    };

    match status {
        button::Status::Active => base,
        button::Status::Hovered | button::Status::Pressed => base.with_background(colors::HOVER),
        button::Status::Disabled => dimmed(base),
    }
}

pub fn danger_button(_theme: &Theme, status: button::Status) -> button::Style {
    let base = button::Style {
        background: None,
        text_color: colors::DANGER,
        border: Border {
            color: colors::DANGER_BORDER,
            width: 1.0,
            radius: radii::CONTROL.into(),
        },
        ..button::Style::default()
    };

    match status {
        button::Status::Active => base,
        button::Status::Hovered | button::Status::Pressed => base.with_background(colors::HOVER),
        button::Status::Disabled => dimmed(base),
    }
}

pub fn primary_button(_theme: &Theme, status: button::Status) -> button::Style {
    let base = button::Style {
        background: Some(colors::PRIMARY.into()),
        text_color: colors::ON_PRIMARY,
        border: Border {
            color: colors::PRIMARY,
            width: 0.0,
            radius: radii::CONTROL.into(),
        },
        ..button::Style::default()
    };

    match status {
        button::Status::Disabled => dimmed(base),
        _ => base,
    }
}

fn dimmed(style: button::Style) -> button::Style {
    button::Style {
        text_color: style.text_color.scale_alpha(0.5),
        border: Border {
            color: style.border.color.scale_alpha(0.5),
            ..style.border
        },
        background: style.background.map(|bg| bg.scale_alpha(0.5)),
        ..style
    }
}

pub fn list_row(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let base = button::Style {
            background: if selected {
                Some(colors::SELECTED.into())
            } else {
                None
            },
            text_color: colors::TEXT,
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: radii::CONTROL.into(),
            },
            ..button::Style::default()
        };

        match status {
            _ if selected => base,
            button::Status::Active => base,
            button::Status::Hovered | button::Status::Pressed => {
                base.with_background(colors::HOVER)
            }
            button::Status::Disabled => dimmed(base),
        }
    }
}

pub fn tag(border: Color, text: Color) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        text_color: Some(text),
        border: Border {
            color: border,
            width: 1.0,
            radius: radii::TAG.into(),
        },
        ..container::Style::default()
    }
}

/// The small square +/− buttons in the file list. Hover uses PILL rather than HOVER
/// because they sit on rows that already turn HOVER when the mouse is over them.
pub fn icon_button(_theme: &Theme, status: button::Status) -> button::Style {
    let base = button::Style {
        background: None,
        text_color: colors::TEXT_MUTED,
        border: Border {
            radius: radii::TAG.into(),
            ..Border::default()
        },
        ..button::Style::default()
    };

    match status {
        button::Status::Active => base,
        button::Status::Hovered | button::Status::Pressed => base.with_background(colors::PILL),
        button::Status::Disabled => dimmed(base),
    }
}

pub fn commit_input(_theme: &Theme, status: text_editor::Status) -> text_editor::Style {
    let border_color = match status {
        text_editor::Status::Focused { .. } => colors::FOCUS,
        _ => colors::CONTROL_BORDER,
    };

    text_editor::Style {
        background: colors::BACKGROUND.into(),
        border: Border {
            color: border_color,
            width: 1.0,
            radius: radii::CONTROL.into(),
        },
        placeholder: colors::TEXT_DIM,
        value: colors::TEXT,
        selection: colors::PRIMARY.scale_alpha(0.4),
    }
}

/// The mockup's 10px scrollbar with a 2px inset: a 6px thumb, 2px from the edge.
pub fn thin_scrollbar() -> scrollable::Scrollbar {
    scrollable::Scrollbar::new()
        .width(6)
        .scroller_width(6)
        .margin(2)
}

/// Every scrollbar in the app: no track, a PILL-grey thumb that lightens on hover/drag.
pub fn scrollbar(theme: &Theme, status: scrollable::Status) -> scrollable::Style {
    let rail = |active: bool| scrollable::Rail {
        background: None,
        border: Border::default(),
        scroller: scrollable::Scroller {
            background: if active {
                colors::SCROLLBAR_HOVER.into()
            } else {
                colors::PILL.into()
            },
            border: Border {
                radius: 3.0.into(),
                ..Border::default()
            },
        },
    };

    let (horizontal_active, vertical_active) = match status {
        scrollable::Status::Active { .. } => (false, false),
        scrollable::Status::Hovered {
            is_horizontal_scrollbar_hovered,
            is_vertical_scrollbar_hovered,
            ..
        } => (is_horizontal_scrollbar_hovered, is_vertical_scrollbar_hovered),
        scrollable::Status::Dragged {
            is_horizontal_scrollbar_dragged,
            is_vertical_scrollbar_dragged,
            ..
        } => (is_horizontal_scrollbar_dragged, is_vertical_scrollbar_dragged),
    };

    scrollable::Style {
        vertical_rail: rail(vertical_active),
        horizontal_rail: rail(horizontal_active),
        gap: None,
        ..scrollable::default(theme, status)
    }
}

/// A filled, square-ish label like the diff header's "Unstaged".
pub fn filled_tag(_theme: &Theme) -> container::Style {
    container::Style {
        text_color: Some(colors::TEXT),
        background: Some(colors::PILL.into()),
        border: Border {
            radius: radii::TAG.into(),
            ..Border::default()
        },
        ..container::Style::default()
    }
}

pub fn pill(_theme: &Theme) -> container::Style {
    container::Style {
        text_color: Some(colors::TEXT),
        background: Some(colors::PILL.into()),
        border: Border {
            radius: radii::PILL.into(),
            ..Border::default()
        },
        ..container::Style::default()
    }
}

pub fn app_theme() -> Theme {
    Theme::custom(
        "Git Reviewer",
        Palette {
            background: colors::BACKGROUND,
            text: colors::TEXT,
            primary: colors::PRIMARY,
            success: colors::ADDED,
            warning: colors::MODIFIED,
            danger: colors::DANGER,
        },
    )
}
