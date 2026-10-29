//! Draws what it holds moved and scaled, without changing its place in the
//! layout: what slides, hops or grows in the dock.
//!
//! The pointer is moved back the same way, so what moves still answers
//! where it shows. A part of the bounds can be cut away: the launcher comes
//! out from behind its bar, not over it.

use iced::advanced::widget::{Operation, Tree, tree};
use iced::advanced::{Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer};
use iced::{
    Element, Event, Length, Padding, Rectangle, Renderer, Size, Theme, Transformation, Vector,
};

/// `content`, drawn `offset` away from its place.
pub fn shift<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
    offset: Vector,
) -> Shift<'a, Message> {
    Shift {
        content: content.into(),
        offset,
        scale: 1.0,
        cut: None,
    }
}

pub struct Shift<'a, Message> {
    content: Element<'a, Message>,
    offset: Vector,
    /// Around its middle.
    scale: f32,
    /// What is cut from its bounds: nothing shows there.
    cut: Option<Padding>,
}

impl<'a, Message: 'a> Shift<'a, Message> {
    pub fn scale(mut self, scale: f32) -> Self {
        self.scale = scale;
        self
    }

    /// Shows nothing of it in these margins of its bounds.
    pub fn cut(mut self, cut: Padding) -> Self {
        self.cut = Some(cut);
        self
    }

    fn still(&self) -> bool {
        self.offset == Vector::ZERO && self.scale == 1.0
    }

    fn transformation(&self, bounds: Rectangle) -> Transformation {
        let center = bounds.center();
        Transformation::translate(center.x + self.offset.x, center.y + self.offset.y)
            * Transformation::scale(self.scale)
            * Transformation::translate(-center.x, -center.y)
    }
}

impl<'a, Message: 'a> From<Shift<'a, Message>> for Element<'a, Message> {
    fn from(shift: Shift<'a, Message>) -> Self {
        Element::new(shift)
    }
}

impl<Message> Widget<Message, Theme, Renderer> for Shift<'_, Message> {
    fn tag(&self) -> tree::Tag {
        self.content.as_widget().tag()
    }

    fn state(&self) -> tree::State {
        self.content.as_widget().state()
    }

    fn children(&self) -> Vec<Tree> {
        self.content.as_widget().children()
    }

    fn diff(&self, tree: &mut Tree) {
        self.content.as_widget().diff(tree);
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn size_hint(&self) -> Size<Length> {
        self.content.as_widget().size_hint()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content.as_widget_mut().layout(tree, renderer, limits)
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
        use iced::advanced::Renderer as _;

        let content = self.content.as_widget();
        if self.still() && self.cut.is_none() {
            content.draw(tree, renderer, theme, style, layout, cursor, viewport);
            return;
        }
        let bounds = layout.bounds();
        let transformation = self.transformation(bounds);
        let inverse = transformation.inverse();
        // What shows: what it holds clips itself to its viewport, iced not
        // narrowing a layer inside another to it.
        let shown = match self.cut {
            Some(cut) => match bounds.shrink(cut).intersection(viewport) {
                Some(shown) => shown,
                None => return,
            },
            None => *viewport,
        };
        let draw = |renderer: &mut Renderer| {
            renderer.with_transformation(transformation, |renderer| {
                content.draw(
                    tree,
                    renderer,
                    theme,
                    style,
                    layout,
                    cursor * inverse,
                    &(shown * inverse),
                );
            });
        };
        if self.cut.is_some() {
            renderer.with_layer(shown, draw);
        } else {
            draw(renderer);
        }
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content
            .as_widget_mut()
            .operate(tree, layout, renderer, operation);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let inverse = self.transformation(layout.bounds()).inverse();
        self.content.as_widget_mut().update(
            tree,
            event,
            layout,
            cursor * inverse,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let inverse = self.transformation(layout.bounds()).inverse();
        self.content.as_widget().mouse_interaction(
            tree,
            layout,
            cursor * inverse,
            viewport,
            renderer,
        )
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        // Menus open where the content is, once it is there.
        self.content.as_widget_mut().overlay(
            tree,
            layout,
            renderer,
            viewport,
            translation + self.offset,
        )
    }
}
