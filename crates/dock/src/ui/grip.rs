//! The edge of the launcher that is dragged to resize it, at the pace of
//! the screen rather than of the mouse.
//!
//! Every message makes `iced_layershell` build the view of every panel again
//! and draw them all: a mouse sending a thousand moves a second kept the
//! dock that busy. While the edge is held, the grip only remembers where the
//! pointer is, and tells it once a frame, when the launcher is drawn.

use iced::advanced::widget::{Tree, tree};
use iced::advanced::{Clipboard, Layout, Shell, Widget, layout, mouse, renderer};
use iced::{Element, Event, Length, Point, Rectangle, Renderer, Size, Theme, window};

/// Wraps `content`: a press on it starts a resize (`start`), the pointer
/// then moves it (`to`, at most once a frame) until the button is let go
/// (`end`). `held` says whether a resize is going on.
pub fn grip<'a, Message: Clone + 'a>(
    content: impl Into<Element<'a, Message>>,
    held: bool,
    start: Message,
    to: fn(Point) -> Message,
    end: Message,
    interaction: mouse::Interaction,
) -> Element<'a, Message> {
    Element::new(Grip {
        content: content.into(),
        held,
        start,
        to,
        end,
        interaction,
    })
}

struct Grip<'a, Message> {
    content: Element<'a, Message>,
    held: bool,
    start: Message,
    to: fn(Point) -> Message,
    end: Message,
    interaction: mouse::Interaction,
}

/// Where the pointer went since the launcher was last told.
#[derive(Default)]
struct State {
    pending: Option<Point>,
}

impl<Message: Clone> Widget<Message, Theme, Renderer> for Grip<'_, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State>();
        if !self.held {
            state.pending = None;
            let pressed = matches!(
                event,
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
            );
            if pressed && cursor.is_over(layout.bounds()) {
                shell.publish(self.start.clone());
                shell.capture_event();
            }
            return;
        }
        // The button held, the launcher keeps the pointer wherever it goes:
        // the moves come here even off the grip.
        match event {
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                state.pending = Some(*position);
                shell.request_redraw();
            }
            Event::Window(window::Event::RedrawRequested(_)) => {
                if let Some(position) = state.pending.take() {
                    shell.publish((self.to)(position));
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                // Where it was let go counts, even inside the last frame.
                if let Some(position) = state.pending.take() {
                    shell.publish((self.to)(position));
                }
                shell.publish(self.end.clone());
                shell.capture_event();
            }
            _ => {}
        }
    }

    fn mouse_interaction(
        &self,
        _tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        if self.held || cursor.is_over(layout.bounds()) {
            self.interaction
        } else {
            mouse::Interaction::None
        }
    }
}
