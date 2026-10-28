//! What GPUI's elements leave out for themes: cutting a layer down to part
//! of its box, and pictures placed by `fit` about a focus point.

use std::sync::Arc;

use gpui_kit::{
    AnyElement, App, Bounds, ContentMask, Corners, Element, ElementId, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, Pixels, RenderImage, Styled as _, Window, canvas,
    point, px, size,
};

use crate::themes::Fit;

/// A layer cut down to part of its own box: `cut` is the share of its
/// height and width taken from the top, right, bottom and left.
pub struct Clip {
    child: AnyElement,
    cut: [f32; 4],
}

pub fn clip(child: AnyElement, cut: [f32; 4]) -> Clip {
    Clip { child, cut }
}

impl IntoElement for Clip {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for Clip {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        (self.child.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.child.prepaint(window, cx);
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let [top, right, bottom, left] = self.cut;
        let (width, height) = (bounds.size.width, bounds.size.height);
        let kept = Bounds::new(
            point(
                bounds.origin.x + width * left,
                bounds.origin.y + height * top,
            ),
            size(
                (width * (1.0 - left - right)).max(px(0.0)),
                (height * (1.0 - top - bottom)).max(px(0.0)),
            ),
        );
        window.with_content_mask(Some(ContentMask { bounds: kept }), |window| {
            self.child.paint(window, cx)
        });
    }
}

/// A picture filling its element as `fit` says, a cover crop keeping
/// `focus` (shares of its width and height) in view.
pub fn picture(
    image: Arc<RenderImage>,
    fit: Fit,
    focus: (f32, f32),
    radius: Corners<Pixels>,
) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let drawn = placed(bounds, &image, fit, focus);
            let _ = window.paint_image(bounds, drawn, radius, image.clone(), 0, false);
        },
    )
    .size_full()
}

/// Where a picture lies for `fit` in `bounds`.
fn placed(
    bounds: Bounds<Pixels>,
    image: &RenderImage,
    fit: Fit,
    focus: (f32, f32),
) -> Bounds<Pixels> {
    let pixels = image.size(0);
    let aspect = if pixels.height.0 > 0 {
        pixels.width.0 as f32 / pixels.height.0 as f32
    } else {
        1.0
    };
    let (width, height) = (bounds.size.width.as_f32(), bounds.size.height.as_f32());
    if fit == Fit::Stretch || width <= 0.0 || height <= 0.0 {
        return bounds;
    }
    let wider = width / height > aspect;
    let (drawn_w, drawn_h) = match (fit, wider) {
        (Fit::Contain, true) | (Fit::Cover, false) => (height * aspect, height),
        _ => (width, width / aspect),
    };
    let (fx, fy) = if fit == Fit::Cover { focus } else { (0.5, 0.5) };
    Bounds::new(
        point(
            bounds.origin.x + px((width - drawn_w) * fx),
            bounds.origin.y + px((height - drawn_h) * fy),
        ),
        size(px(drawn_w), px(drawn_h)),
    )
}
