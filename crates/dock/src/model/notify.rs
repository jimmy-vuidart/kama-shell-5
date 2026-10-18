//! Session notification queue: history, replace, toasts, and do-not-disturb.
//! Pure logic; D-Bus and drawing live elsewhere.

use coquille_model::OutputRole;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use super::{Orientation, Side};

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
pub enum CloseReason {
    Expired = 1,
    Dismissed = 2,
    Requested = 3,
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
    /// When the notice was received, in UTC seconds since the epoch.
    pub received: i64,
    /// The `desktop-entry` hint: the application, as its `.desktop` file
    /// names it.
    pub desktop_entry: Option<String>,
    /// The `transient` hint: gone with its toast, never kept in history.
    pub transient: bool,
    /// The `resident` hint: invoking an action does not close it.
    pub resident: bool,
}

impl Notice {
    pub fn sanitize(mut self) -> Self {
        self.app = clip(&self.app, MAX_APP);
        self.desktop_entry = self
            .desktop_entry
            .map(|entry| clip(&entry, MAX_APP))
            .filter(|entry| !entry.is_empty());
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
///
/// The server advertises `persistence`: a notice stays open, in history,
/// once its toast is gone, until the user acts on it or dismisses it, or
/// the application closes it. History therefore only holds open notices,
/// and closing one removes it.
#[derive(Debug, Clone, Default)]
pub struct NotifyState {
    /// Newest first.
    pub history: Vec<Notice>,
    pub visible: Vec<u32>,
    pub user_dnd: bool,
    next_id: u32,
    shown_at: HashMap<u32, Instant>,
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
        self.history.insert(0, notice);
        self.history.truncate(MAX_HISTORY);

        let any_anchored = !anchored.is_empty();
        let show = !self.effective_dnd(any_anchored) && self.toast_bar(bars, anchored).is_some();
        self.visible.retain(|visible| *visible != id);
        self.shown_at.remove(&id);
        if show {
            self.visible.insert(0, id);
            self.visible.truncate(MAX_VISIBLE);
            self.shown_at.insert(id, Instant::now());
            let kept: HashSet<_> = self.visible.iter().copied().collect();
            self.shown_at.retain(|shown, _| kept.contains(shown));
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
        self.shown_at.remove(&id);
        before != self.visible.len()
    }

    /// Drops every toast without closing the notices (DND / anchored screen).
    pub fn hide_toasts(&mut self) {
        self.visible.clear();
        self.shown_at.clear();
    }

    /// Visible notices, newest first.
    pub fn visible_notices(&self) -> Vec<&Notice> {
        self.visible
            .iter()
            .filter_map(|id| self.notice(*id))
            .collect()
    }

    #[cfg(test)]
    pub fn shown_at(&self, id: u32) -> Option<Instant> {
        self.shown_at.get(&id).copied()
    }

    /// Toasts whose timeout has elapsed. Sticky notices (`expire_ms` none) never appear.
    pub fn expire_due(&self, now: Instant) -> Vec<u32> {
        self.visible
            .iter()
            .copied()
            .filter(|id| {
                let Some(notice) = self.notice(*id) else {
                    return false;
                };
                let Some(ms) = notice.expire_ms else {
                    return false;
                };
                let Some(shown) = self.shown_at.get(id) else {
                    return false;
                };
                now.saturating_duration_since(*shown) >= Duration::from_millis(u64::from(ms))
            })
            .collect()
    }

    /// Hides the toasts whose time is up. Their notices stay open in
    /// history, except transient ones: those are closed, and their ids
    /// returned to emit `Expired`.
    pub fn expire(&mut self, now: Instant) -> Vec<u32> {
        let mut closed = Vec::new();
        for id in self.expire_due(now) {
            if self.notice(id).is_some_and(|notice| notice.transient) {
                self.close(id);
                closed.push(id);
            } else {
                self.hide(id);
            }
        }
        closed
    }

    /// Closes an open notification: its toast and its line in history go.
    /// `false` when it was not open (already closed, or never known), so a
    /// close is only ever signalled once.
    pub fn close(&mut self, id: u32) -> bool {
        let had = self.history.iter().any(|notice| notice.id == id);
        self.history.retain(|notice| notice.id != id);
        self.hide(id);
        had
    }

    /// Closes every notification; returns the ids that were open.
    pub fn clear(&mut self) -> Vec<u32> {
        let ids = self.history.iter().map(|notice| notice.id).collect();
        self.history.clear();
        self.visible.clear();
        self.shown_at.clear();
        ids
    }

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

/// Gap between the toast stack and the dock / screen edge. The compositor
/// already keeps the surface out of the dock's reserved zone.
pub const TOAST_EDGE_GAP: i32 = 8;
pub const TOAST_STACK_GAP: u32 = 8;
pub const TOAST_CARD_W_VERTICAL: u32 = 260;
pub const TOAST_CARD_H_VERTICAL: u32 = 108;
pub const TOAST_CARD_W_HORIZONTAL: u32 = 220;
pub const TOAST_CARD_H_HORIZONTAL: u32 = 96;

/// Which corner of the screen the overlay is glued to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastAnchor {
    /// Upright dock on the left: pile grows up from the bottom.
    LeftBottom,
    /// Upright dock on the right.
    RightBottom,
    /// Lying dock: row grows left from the bottom-right.
    BottomRight,
}

/// Overlay size, anchors and layer-shell margins (`top, right, bottom, left`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToastGeometry {
    pub width: u32,
    pub height: u32,
    pub anchor: ToastAnchor,
    pub margin: (i32, i32, i32, i32),
}

/// Surface that holds `count` cards at the end of a dock of `orientation`.
pub fn toast_geometry(orientation: Orientation, count: usize) -> ToastGeometry {
    let count = count.clamp(1, MAX_VISIBLE) as u32;
    let along_dock = TOAST_EDGE_GAP;
    match orientation {
        Orientation::Vertical(Side::Left) => ToastGeometry {
            width: TOAST_CARD_W_VERTICAL,
            height: stack_len(TOAST_CARD_H_VERTICAL, count),
            anchor: ToastAnchor::LeftBottom,
            margin: (0, 0, TOAST_EDGE_GAP, along_dock),
        },
        Orientation::Vertical(Side::Right) => ToastGeometry {
            width: TOAST_CARD_W_VERTICAL,
            height: stack_len(TOAST_CARD_H_VERTICAL, count),
            anchor: ToastAnchor::RightBottom,
            margin: (0, along_dock, TOAST_EDGE_GAP, 0),
        },
        Orientation::Horizontal => ToastGeometry {
            width: stack_len(TOAST_CARD_W_HORIZONTAL, count),
            height: TOAST_CARD_H_HORIZONTAL,
            anchor: ToastAnchor::BottomRight,
            margin: (0, TOAST_EDGE_GAP, along_dock, 0),
        },
    }
}

fn stack_len(card: u32, count: u32) -> u32 {
    card * count + TOAST_STACK_GAP * count.saturating_sub(1)
}

/// What `sync_toasts` should do to the overlay surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastSurfacePlan {
    Keep,
    Resize { size: (u32, u32) },
    Relocate,
    Open,
    Close,
}

/// Pure overlay lifecycle: open, resize, relocate, or close.
///
/// `open` is the bar, orientation and last geometry of an existing surface.
/// `wanted` is the bar that should host toasts (`None` when DND, anchoring,
/// or no visible bar).
pub fn toast_surface_plan(
    open: Option<(OutputRole, Orientation, ToastGeometry)>,
    wanted: Option<(OutputRole, Orientation)>,
    visible: usize,
) -> ToastSurfacePlan {
    match (wanted, open) {
        (None, None) => ToastSurfacePlan::Keep,
        (None, Some(_)) | (Some(_), Some(_)) if visible == 0 => ToastSurfacePlan::Close,
        (None, Some(_)) => ToastSurfacePlan::Close,
        (Some(_), None) => ToastSurfacePlan::Open,
        (Some((screen, orientation)), Some((open_screen, open_orientation, geo))) => {
            let next = toast_geometry(orientation, visible);
            if screen != open_screen
                || orientation != open_orientation
                || next.anchor != geo.anchor
                || next.margin != geo.margin
            {
                ToastSurfacePlan::Relocate
            } else if (next.width, next.height) != (geo.width, geo.height) {
                ToastSurfacePlan::Resize {
                    size: (next.width, next.height),
                }
            } else {
                ToastSurfacePlan::Keep
            }
        }
    }
}

/// Markup as plain text for iced labels.
pub fn plain_text(markup: &str) -> String {
    let mut out = String::with_capacity(markup.len());
    let mut rest = markup;
    while !rest.is_empty() {
        if rest.starts_with('<') {
            if let Some(end) = rest.find('>') {
                rest = &rest[end + 1..];
                continue;
            }
            rest = &rest[1..];
            continue;
        }
        if rest.starts_with('&') {
            if let Some(plain) = entity_plain(rest) {
                out.push_str(plain.0);
                rest = &rest[plain.1..];
                continue;
            }
            if let Some(entity) = numeric_entity(rest) {
                if let Some(ch) = numeric_char(entity) {
                    out.push(ch);
                }
                rest = &rest[entity.len()..];
                continue;
            }
            out.push('&');
            rest = &rest[1..];
            continue;
        }
        let ch = rest.chars().next().unwrap();
        out.push(ch);
        rest = &rest[ch.len_utf8()..];
    }
    out
}

fn entity_plain(rest: &str) -> Option<(&'static str, usize)> {
    const NAMED: &[(&str, &str)] = &[
        ("&amp;", "&"),
        ("&lt;", "<"),
        ("&gt;", ">"),
        ("&apos;", "'"),
        ("&quot;", "\""),
    ];
    for (entity, plain) in NAMED {
        if rest.starts_with(entity) {
            return Some((plain, entity.len()));
        }
    }
    None
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

/// The character a well-formed numeric reference stands for.
fn numeric_char(entity: &str) -> Option<char> {
    let digits = entity.strip_prefix("&#")?.strip_suffix(';')?;
    let code = match digits.strip_prefix(['x', 'X']) {
        Some(hex) => u32::from_str_radix(hex, 16).ok()?,
        None => digits.parse().ok()?,
    };
    char::from_u32(code)
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
            received: 0,
            desktop_entry: None,
            transient: false,
            resident: false,
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
    fn close_is_one_time_and_leaves_history() {
        let mut state = NotifyState::new();
        state.ingest(notice(7, "known"), &left_bar(), &[]);
        state.ingest(notice(8, "other"), &left_bar(), &[]);
        assert_eq!(state.visible, vec![8, 7]);

        assert!(state.close(7));
        assert_eq!(state.visible, vec![8]);
        assert!(state.notice(7).is_none());
        assert_eq!(state.history.len(), 1);
        assert!(!state.close(7));
    }

    #[test]
    fn a_closed_id_sent_again_opens_a_new_notice() {
        let mut state = NotifyState::new();
        let bars = left_bar();
        state.ingest(notice(7, "known"), &bars, &[]);
        assert!(state.close(7));

        assert_eq!(
            state.ingest(notice(7, "updated"), &bars, &[]),
            ToastDecision::Shown
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
    }

    #[test]
    fn clear_closes_every_open_notice_once() {
        let mut state = NotifyState::new();
        state.ingest(notice(0, "one"), &left_bar(), &[]);
        state.ingest(notice(0, "two"), &left_bar(), &[]);
        let (newest, oldest) = (state.history[0].id, state.history[1].id);
        assert!(state.close(oldest));
        assert_eq!(state.clear(), vec![newest]);
        assert!(state.history.is_empty());
        assert!(state.visible.is_empty());
        assert!(state.clear().is_empty());
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

    #[test]
    fn sticky_toast_never_expires() {
        let mut state = NotifyState::new();
        let mut sticky = notice(0, "stay");
        sticky.expire_ms = None;
        state.ingest(sticky, &left_bar(), &[]);
        let id = state.visible[0];
        let later = state.shown_at(id).unwrap() + Duration::from_secs(60);
        assert!(state.expire_due(later).is_empty());
        assert!(state.expire(later).is_empty());
        assert_eq!(state.visible, vec![id]);
    }

    #[test]
    fn an_expired_toast_leaves_its_notice_open_in_history() {
        let mut state = NotifyState::new();
        let mut short = notice(0, "bye");
        short.expire_ms = Some(50);
        state.ingest(short, &left_bar(), &[]);
        let id = state.visible[0];
        let shown = state.shown_at(id).unwrap();
        assert!(
            state
                .expire_due(shown + Duration::from_millis(49))
                .is_empty()
        );
        assert_eq!(
            state.expire_due(shown + Duration::from_millis(50)),
            vec![id]
        );
        assert!(state.expire(shown + Duration::from_millis(50)).is_empty());
        assert!(state.visible.is_empty());
        assert!(state.notice(id).is_some());
        // Still open: dismissing it from history closes it.
        assert!(state.close(id));
    }

    #[test]
    fn an_expired_transient_toast_is_closed() {
        let mut state = NotifyState::new();
        let mut passing = notice(0, "volume");
        passing.expire_ms = Some(50);
        passing.transient = true;
        state.ingest(passing, &left_bar(), &[]);
        let id = state.visible[0];
        let later = state.shown_at(id).unwrap() + Duration::from_millis(50);
        assert_eq!(state.expire(later), vec![id]);
        assert!(state.history.is_empty());
        assert!(state.expire(later).is_empty());
    }

    #[test]
    fn hide_toasts_keeps_history_open() {
        let mut state = NotifyState::new();
        state.ingest(notice(0, "hi"), &left_bar(), &[]);
        let id = state.visible[0];
        state.hide_toasts();
        assert!(state.visible.is_empty());
        assert!(state.notice(id).is_some());
        assert!(state.close(id));
    }

    #[test]
    fn toast_geometry_follows_bar_orientation() {
        let left = toast_geometry(Orientation::Vertical(Side::Left), 2);
        assert_eq!(left.anchor, ToastAnchor::LeftBottom);
        assert_eq!(left.width, TOAST_CARD_W_VERTICAL);
        assert_eq!(left.height, TOAST_CARD_H_VERTICAL * 2 + TOAST_STACK_GAP);
        assert_eq!(left.margin, (0, 0, TOAST_EDGE_GAP, TOAST_EDGE_GAP));

        let right = toast_geometry(Orientation::Vertical(Side::Right), 1);
        assert_eq!(right.anchor, ToastAnchor::RightBottom);
        assert_eq!(right.margin, (0, TOAST_EDGE_GAP, TOAST_EDGE_GAP, 0));

        let bottom = toast_geometry(Orientation::Horizontal, 3);
        assert_eq!(bottom.anchor, ToastAnchor::BottomRight);
        assert_eq!(bottom.height, TOAST_CARD_H_HORIZONTAL);
        assert_eq!(
            bottom.width,
            TOAST_CARD_W_HORIZONTAL * 3 + TOAST_STACK_GAP * 2
        );
        assert_eq!(bottom.margin, (0, TOAST_EDGE_GAP, TOAST_EDGE_GAP, 0));

        let capped = toast_geometry(Orientation::Horizontal, 9);
        assert_eq!(capped.width, bottom.width);
    }

    #[test]
    fn toast_surface_plan_opens_resizes_relocates_and_closes() {
        let left = (OutputRole::Left, Orientation::Vertical(Side::Left));
        let right = (OutputRole::Right, Orientation::Vertical(Side::Right));
        let geo = toast_geometry(left.1, 1);
        assert_eq!(toast_surface_plan(None, None, 0), ToastSurfacePlan::Keep);
        assert_eq!(
            toast_surface_plan(None, Some(left), 1),
            ToastSurfacePlan::Open
        );
        assert_eq!(
            toast_surface_plan(Some((left.0, left.1, geo)), None, 1),
            ToastSurfacePlan::Close
        );
        assert_eq!(
            toast_surface_plan(Some((left.0, left.1, geo)), Some(left), 0),
            ToastSurfacePlan::Close
        );

        let two = toast_geometry(left.1, 2);
        assert_eq!(
            toast_surface_plan(Some((left.0, left.1, geo)), Some(left), 2),
            ToastSurfacePlan::Resize {
                size: (two.width, two.height)
            }
        );
        assert_eq!(
            toast_surface_plan(Some((left.0, left.1, geo)), Some(right), 1),
            ToastSurfacePlan::Relocate
        );
        let lying = (OutputRole::Left, Orientation::Horizontal);
        assert_eq!(
            toast_surface_plan(Some((left.0, left.1, geo)), Some(lying), 1),
            ToastSurfacePlan::Relocate
        );
        assert_eq!(
            toast_surface_plan(Some((left.0, left.1, geo)), Some(left), 1),
            ToastSurfacePlan::Keep
        );
    }

    #[test]
    fn plain_text_strips_markup() {
        assert_eq!(plain_text("<b>ok</b> &amp; <i>and</i>"), "ok & and");
        assert_eq!(
            plain_text("j&#39;arrive &#x2014;&#xFFFFFFF; ok"),
            "j'arrive — ok"
        );
        assert_eq!(plain_text(&sanitize_markup("l'appli")), "l'appli");
    }
}
