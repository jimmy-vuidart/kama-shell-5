//! Session notification queue: history, replace, toasts, and do-not-disturb.
//! Pure logic; D-Bus and drawing live elsewhere.

use coquille_model::OutputRole;
use std::collections::HashSet;

use super::Orientation;

/// Client timeout omitted (`-1`): hide the toast after this many milliseconds.
pub const DEFAULT_EXPIRE_MS: u32 = 5_000;
/// Newest toasts kept on screen; the rest stay in history.
pub const MAX_VISIBLE: usize = 3;
const MAX_SUMMARY: usize = 512;
const MAX_BODY: usize = 4_096;
const MAX_APP: usize = 256;
const MAX_ACTION: usize = 128;
const MAX_HISTORY: usize = 200;
/// Pixel budget for `image-data` (and similar) payloads.
pub const MAX_IMAGE_PIXELS: u32 = 512 * 512;

/// How a notification was closed, as `org.freedesktop.Notifications` reasons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
#[allow(dead_code)] // expired/dismissed are emitted once toasts exist
pub enum CloseReason {
    Expired = 1,
    Dismissed = 2,
    Requested = 3,
    Undefined = 4,
}

impl CloseReason {
    pub const fn as_u32(self) -> u32 {
        self as u32
    }
}

/// Where the icon comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Icon {
    /// Freedesktop icon name or a path the client sent as `app_icon`.
    Named(String),
    /// A file path from `image-path` / `image_path`.
    File(String),
    /// Decoded `image-data` (RGBA8).
    Pixels {
        width: u32,
        height: u32,
        rgba: Vec<u8>,
    },
}

/// One notification, as the dock stores it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub id: u32,
    pub app: String,
    pub summary: String,
    pub body: String,
    pub icon: Option<Icon>,
    /// `(key, label)` pairs; `default` is the body click.
    pub actions: Vec<(String, String)>,
    /// `None` means sticky (timeout 0).
    pub expire_ms: Option<u32>,
    pub image: Option<Icon>,
}

impl Notice {
    pub fn sanitize(mut self) -> Self {
        self.app = clip(&self.app, MAX_APP);
        self.summary = clip(&self.summary, MAX_SUMMARY);
        self.body = sanitize_markup(&clip(&self.body, MAX_BODY));
        self.actions = self
            .actions
            .into_iter()
            .map(|(key, label)| (clip(&key, MAX_ACTION), clip(&label, MAX_ACTION)))
            .filter(|(key, _)| !key.is_empty())
            .collect();
        self
    }
}

/// Whether a newly ingested notice should show a toast.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastDecision {
    Shown,
    HistoryOnly,
    Replaced { id: u32 },
}

/// In-memory session notifications.
#[derive(Debug, Clone, Default)]
pub struct NotifyState {
    /// Newest first.
    pub history: Vec<Notice>,
    pub visible: Vec<u32>,
    pub user_dnd: bool,
    next_id: u32,
    closed: HashSet<u32>,
}

impl NotifyState {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            ..Self::default()
        }
    }

    pub fn effective_dnd(&self, any_anchored: bool) -> bool {
        self.user_dnd || any_anchored
    }

    /// First bar still on a non-anchored screen, left → center → right.
    pub fn toast_bar(
        &self,
        bars: &[(OutputRole, Orientation)],
        anchored: &[OutputRole],
    ) -> Option<(OutputRole, Orientation)> {
        let order = [OutputRole::Left, OutputRole::Center, OutputRole::Right];
        for role in order {
            if anchored.contains(&role) {
                continue;
            }
            if let Some((_, orientation)) = bars.iter().find(|(bar, _)| *bar == role) {
                return Some((role, *orientation));
            }
        }
        None
    }

    pub fn ingest(
        &mut self,
        mut notice: Notice,
        bars: &[(OutputRole, Orientation)],
        anchored: &[OutputRole],
    ) -> ToastDecision {
        notice = notice.sanitize();
        let replacing = notice.id != 0 && self.history.iter().any(|known| known.id == notice.id);
        if notice.id == 0 {
            notice.id = self.allocate();
        } else {
            self.next_id = self.next_id.max(notice.id.saturating_add(1));
        }
        let id = notice.id;

        if replacing {
            self.history.retain(|known| known.id != id);
        }
        self.closed.remove(&id);
        self.history.insert(0, notice);
        self.history.truncate(MAX_HISTORY);
        let retained_ids: HashSet<_> = self.history.iter().map(|notice| notice.id).collect();
        self.closed.retain(|closed| retained_ids.contains(closed));

        let any_anchored = !anchored.is_empty();
        let show = !self.effective_dnd(any_anchored) && self.toast_bar(bars, anchored).is_some();
        self.visible.retain(|visible| *visible != id);
        if show {
            self.visible.insert(0, id);
            self.visible.truncate(MAX_VISIBLE);
            if replacing {
                ToastDecision::Replaced { id }
            } else {
                ToastDecision::Shown
            }
        } else {
            ToastDecision::HistoryOnly
        }
    }

    /// Hides a toast; the line stays in history (session persistence).
    pub fn hide(&mut self, id: u32) -> bool {
        let before = self.visible.len();
        self.visible.retain(|visible| *visible != id);
        before != self.visible.len()
    }

    /// Closes a notification once while retaining its session-history entry.
    pub fn close(&mut self, id: u32) -> bool {
        if !self.history.iter().any(|notice| notice.id == id) || !self.closed.insert(id) {
            return false;
        }
        self.hide(id);
        true
    }

    /// Removes a notice from history and from the toast stack.
    #[allow(dead_code)]
    pub fn forget(&mut self, id: u32) -> bool {
        let had = self.history.iter().any(|notice| notice.id == id);
        self.history.retain(|notice| notice.id != id);
        self.visible.retain(|visible| *visible != id);
        self.closed.remove(&id);
        had
    }

    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.history.clear();
        self.visible.clear();
        self.closed.clear();
    }

    #[allow(dead_code)]
    pub fn notice(&self, id: u32) -> Option<&Notice> {
        self.history.iter().find(|notice| notice.id == id)
    }

    fn allocate(&mut self) -> u32 {
        let id = if self.next_id == 0 { 1 } else { self.next_id };
        self.next_id = id.saturating_add(1);
        if self.next_id == 0 {
            self.next_id = 1;
        }
        id
    }
}

/// Client expire timeout: `0` sticky, negative → default, otherwise milliseconds.
pub fn expire_from_timeout(timeout_ms: i32) -> Option<u32> {
    match timeout_ms {
        0 => None,
        n if n < 0 => Some(DEFAULT_EXPIRE_MS),
        n => Some(n as u32),
    }
}

/// Flattened D-Bus `actions` array → `(key, label)` pairs.
pub fn actions_from_flat(flat: &[String]) -> Vec<(String, String)> {
    flat.chunks(2)
        .filter_map(|chunk| {
            let key = chunk.first()?.clone();
            let label = chunk.get(1).cloned().unwrap_or_default();
            (!key.is_empty()).then_some((key, label))
        })
        .collect()
}

/// Allows only `<b>`, `<i>`, `<u>`, named XML entities, and numeric character references.
/// Unknown tags are dropped (their inner text is kept); stray `<` / `>` are escaped.
pub fn sanitize_markup(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while !rest.is_empty() {
        if rest.starts_with('&') {
            if let Some(entity) = known_entity(rest) {
                out.push_str(entity);
                rest = &rest[entity.len()..];
                continue;
            }
            out.push_str("&amp;");
            rest = &rest[1..];
            continue;
        }
        if rest.starts_with('<') {
            if let Some((tag, skip)) = allowed_tag(rest) {
                out.push_str(tag);
                rest = &rest[skip..];
                continue;
            }
            if let Some(skip) = skip_unknown_tag(rest) {
                rest = &rest[skip..];
                continue;
            }
            out.push_str("&lt;");
            rest = &rest[1..];
            continue;
        }
        let ch = rest.chars().next().unwrap();
        match ch {
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
        rest = &rest[ch.len_utf8()..];
    }
    out
}

fn known_entity(rest: &str) -> Option<&str> {
    for named in ["&amp;", "&lt;", "&gt;", "&apos;", "&quot;"] {
        if rest.starts_with(named) {
            return Some(named);
        }
    }
    numeric_entity(rest)
}

/// Well-formed `&#123;` / `&#x1F;` (GLib and Chromium emit `&#39;` for `'`).
fn numeric_entity(rest: &str) -> Option<&str> {
    let digits = rest.strip_prefix("&#")?;
    let (digits, prefix) = if let Some(hex) = digits.strip_prefix(['x', 'X']) {
        (hex, 3)
    } else {
        (digits, 2)
    };
    let end = digits.find(';')?;
    let body = &digits[..end];
    if body.is_empty() {
        return None;
    }
    let ok = if prefix == 3 {
        body.chars().all(|c| c.is_ascii_hexdigit())
    } else {
        body.chars().all(|c| c.is_ascii_digit())
    };
    ok.then_some(&rest[..prefix + end + 1])
}

fn skip_unknown_tag(rest: &str) -> Option<usize> {
    let inner = rest.strip_prefix('<')?;
    let name = inner.strip_prefix('/').unwrap_or(inner);
    let first = name.chars().next()?;
    if !first.is_ascii_alphabetic() {
        return None;
    }
    rest.find('>').map(|i| i + 1)
}

fn allowed_tag(rest: &str) -> Option<(&str, usize)> {
    ["<b>", "</b>", "<i>", "</i>", "<u>", "</u>"]
        .into_iter()
        .find(|tag| rest.starts_with(tag))
        .map(|tag| (tag, tag.len()))
}

fn clip(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    text.chars().take(max).collect()
}

/// Turns raw Freedesktop `image-data` into RGBA8, or `None` if it is unusable.
pub fn pixels_from_image_data(
    width: i32,
    height: i32,
    rowstride: i32,
    has_alpha: bool,
    bits_per_sample: i32,
    channels: i32,
    data: &[u8],
) -> Option<Icon> {
    if width <= 0 || height <= 0 || rowstride <= 0 {
        return None;
    }
    if bits_per_sample != 8 {
        return None;
    }
    let expected_channels = if has_alpha { 4 } else { 3 };
    if channels != expected_channels {
        return None;
    }
    let width = width as u32;
    let height = height as u32;
    if width.saturating_mul(height) > MAX_IMAGE_PIXELS {
        return None;
    }
    let rowstride = rowstride as usize;
    let channels = channels as usize;
    let row_bytes = (width as usize).checked_mul(channels)?;
    if rowstride < row_bytes {
        return None;
    }
    // libnotify omits padding on the last row: (height-1)*rowstride + width*channels.
    let needed = (height as usize - 1)
        .checked_mul(rowstride)?
        .checked_add(row_bytes)?;
    if data.len() < needed {
        return None;
    }
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height as usize {
        let row = &data[y * rowstride..];
        for x in 0..width as usize {
            let i = x * channels;
            if i + channels > row.len() {
                return None;
            }
            rgba.push(row[i]);
            rgba.push(row[i + 1]);
            rgba.push(row[i + 2]);
            rgba.push(if has_alpha { row[i + 3] } else { 255 });
        }
    }
    Some(Icon::Pixels {
        width,
        height,
        rgba,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notice(id: u32, summary: &str) -> Notice {
        Notice {
            id,
            app: "test".into(),
            summary: summary.into(),
            body: String::new(),
            icon: None,
            actions: Vec::new(),
            expire_ms: Some(DEFAULT_EXPIRE_MS),
            image: None,
        }
    }

    fn left_bar() -> Vec<(OutputRole, Orientation)> {
        vec![(
            OutputRole::Left,
            Orientation::Vertical(super::super::Side::Left),
        )]
    }

    #[test]
    fn replace_id_updates_the_same_notice() {
        let mut state = NotifyState::new();
        let bars = left_bar();
        let first = state.ingest(notice(0, "one"), &bars, &[]);
        assert_eq!(first, ToastDecision::Shown);
        let id = state.visible[0];
        let mut again = notice(id, "two");
        again.body = "updated".into();
        let decision = state.ingest(again, &bars, &[]);
        assert_eq!(decision, ToastDecision::Replaced { id });
        assert_eq!(state.history.len(), 1);
        assert_eq!(state.history[0].summary, "two");
        assert_eq!(state.visible, vec![id]);
    }

    #[test]
    fn timeout_zero_is_sticky() {
        assert_eq!(expire_from_timeout(0), None);
        assert_eq!(expire_from_timeout(-1), Some(DEFAULT_EXPIRE_MS));
        assert_eq!(expire_from_timeout(1200), Some(1200));
        let mut state = NotifyState::new();
        let mut sticky = notice(0, "stay");
        sticky.expire_ms = expire_from_timeout(0);
        state.ingest(sticky, &left_bar(), &[]);
        assert!(state.history[0].expire_ms.is_none());
    }

    #[test]
    fn user_dnd_keeps_history_without_toast() {
        let mut state = NotifyState::new();
        state.user_dnd = true;
        let decision = state.ingest(notice(0, "quiet"), &left_bar(), &[]);
        assert_eq!(decision, ToastDecision::HistoryOnly);
        assert!(state.visible.is_empty());
        assert_eq!(state.history.len(), 1);
        assert!(state.effective_dnd(false));
    }

    #[test]
    fn anchored_screen_falls_back_to_another_bar() {
        let state = NotifyState::new();
        let bars = vec![
            (
                OutputRole::Left,
                Orientation::Vertical(super::super::Side::Left),
            ),
            (OutputRole::Center, Orientation::Horizontal),
        ];
        let bar = state.toast_bar(&bars, &[OutputRole::Left]);
        assert_eq!(bar, Some((OutputRole::Center, Orientation::Horizontal)));
    }

    #[test]
    fn all_bars_anchored_means_history_only() {
        let mut state = NotifyState::new();
        let bars = left_bar();
        let decision = state.ingest(notice(0, "game"), &bars, &[OutputRole::Left]);
        assert_eq!(decision, ToastDecision::HistoryOnly);
        assert!(state.toast_bar(&bars, &[OutputRole::Left]).is_none());
        assert!(state.effective_dnd(true));
        assert!(!state.user_dnd);
    }

    #[test]
    fn toast_bar_prefers_left_then_center() {
        let state = NotifyState::new();
        let bars = vec![
            (OutputRole::Right, Orientation::Horizontal),
            (OutputRole::Center, Orientation::Horizontal),
            (
                OutputRole::Left,
                Orientation::Vertical(super::super::Side::Left),
            ),
        ];
        assert_eq!(
            state.toast_bar(&bars, &[]),
            Some((
                OutputRole::Left,
                Orientation::Vertical(super::super::Side::Left)
            ))
        );
        assert_eq!(
            state.toast_bar(&bars, &[OutputRole::Left]),
            Some((OutputRole::Center, Orientation::Horizontal))
        );
    }

    #[test]
    fn markup_strips_unknown_tags() {
        let cleaned = sanitize_markup("<b>ok</b><script>x</script>& <i>and</i>");
        assert_eq!(cleaned, "<b>ok</b>x&amp; <i>and</i>");
        assert_eq!(
            sanitize_markup("<a href=\"https://example\">lien</a> 2 < 3 >"),
            "lien 2 &lt; 3 &gt;"
        );
        assert_eq!(sanitize_markup("<img src='x'/>ok"), "ok");
    }

    #[test]
    fn markup_keeps_numeric_entities() {
        assert_eq!(sanitize_markup("j&#39;arrive"), "j&#39;arrive");
        assert_eq!(sanitize_markup("&#x27;"), "&#x27;");
        assert_eq!(sanitize_markup("&amp;#39;"), "&amp;#39;");
        assert_eq!(sanitize_markup("&#;"), "&amp;#;");
    }

    #[test]
    fn ingest_with_one_anchored_bar_is_history_only() {
        let mut state = NotifyState::new();
        let bars = vec![
            (
                OutputRole::Left,
                Orientation::Vertical(super::super::Side::Left),
            ),
            (OutputRole::Center, Orientation::Horizontal),
        ];
        let decision = state.ingest(notice(0, "game"), &bars, &[OutputRole::Left]);
        assert_eq!(decision, ToastDecision::HistoryOnly);
        assert!(state.visible.is_empty());
        assert_eq!(
            state.toast_bar(&bars, &[OutputRole::Left]),
            Some((OutputRole::Center, Orientation::Horizontal))
        );
    }

    #[test]
    fn close_unknown_notice_is_ignored() {
        let mut state = NotifyState::new();
        assert!(!state.close(99));
        assert!(state.notice(99).is_none());
    }

    #[test]
    fn close_is_one_time_and_keeps_history() {
        let mut state = NotifyState::new();
        state.ingest(notice(7, "known"), &left_bar(), &[]);
        assert_eq!(state.visible, vec![7]);

        assert!(state.close(7));
        assert!(state.visible.is_empty());
        assert_eq!(state.notice(7).unwrap().summary, "known");
        assert_eq!(state.history.len(), 1);
        assert!(!state.close(7));
        assert_eq!(state.history.len(), 1);
    }

    #[test]
    fn replacing_closed_notice_reopens_its_lifecycle() {
        let mut state = NotifyState::new();
        let bars = left_bar();
        state.ingest(notice(7, "known"), &bars, &[]);
        assert!(state.close(7));

        assert_eq!(
            state.ingest(notice(7, "updated"), &bars, &[]),
            ToastDecision::Replaced { id: 7 }
        );
        assert_eq!(state.visible, vec![7]);
        assert_eq!(state.notice(7).unwrap().summary, "updated");
        assert!(state.close(7));
    }

    #[test]
    fn close_reasons_match_the_spec() {
        assert_eq!(CloseReason::Expired.as_u32(), 1);
        assert_eq!(CloseReason::Dismissed.as_u32(), 2);
        assert_eq!(CloseReason::Requested.as_u32(), 3);
        assert_eq!(CloseReason::Undefined.as_u32(), 4);
    }

    #[test]
    fn forget_and_clear_drop_history() {
        let mut state = NotifyState::new();
        state.ingest(notice(0, "one"), &left_bar(), &[]);
        let id = state.history[0].id;
        assert!(state.notice(id).is_some());
        assert!(state.forget(id));
        assert!(state.history.is_empty());
        state.ingest(notice(0, "two"), &left_bar(), &[]);
        state.clear();
        assert!(state.visible.is_empty());
    }

    #[test]
    fn image_data_rejects_oversized_payloads() {
        let none = pixels_from_image_data(1024, 1024, 1024 * 3, false, 8, 3, &[]);
        assert!(none.is_none());
        let rgb = [10, 20, 30];
        let icon = pixels_from_image_data(1, 1, 3, false, 8, 3, &rgb).unwrap();
        match icon {
            Icon::Pixels {
                width,
                height,
                rgba,
            } => {
                assert_eq!((width, height), (1, 1));
                assert_eq!(rgba, vec![10, 20, 30, 255]);
            }
            _ => panic!("expected pixels"),
        }
    }

    #[test]
    fn image_data_accepts_padded_and_unpadded_last_row() {
        // 2×2 RGB, rowstride 8 (> 6). Last row unpadded would be 8+6 = 14.
        let padded: Vec<u8> = vec![
            1, 2, 3, 4, 5, 6, 0, 0, // row 0
            7, 8, 9, 10, 11, 12, 0, 0, // row 1 padded
        ];
        let icon = pixels_from_image_data(2, 2, 8, false, 8, 3, &padded).unwrap();
        match icon {
            Icon::Pixels { rgba, .. } => {
                assert_eq!(
                    rgba,
                    vec![1, 2, 3, 255, 4, 5, 6, 255, 7, 8, 9, 255, 10, 11, 12, 255]
                );
            }
            _ => panic!("expected pixels"),
        }
        let unpadded = &padded[..14];
        assert!(pixels_from_image_data(2, 2, 8, false, 8, 3, unpadded).is_some());
        assert!(pixels_from_image_data(2, 2, 5, false, 8, 3, unpadded).is_none());
        let rgba_px = [1, 2, 3, 4, 5, 6, 7, 8];
        let icon = pixels_from_image_data(2, 1, 8, true, 8, 4, &rgba_px).unwrap();
        match icon {
            Icon::Pixels { rgba, .. } => assert_eq!(rgba, vec![1, 2, 3, 4, 5, 6, 7, 8]),
            _ => panic!("expected pixels"),
        }
    }
}
