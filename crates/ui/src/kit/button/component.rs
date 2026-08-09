use dioxus::prelude::*;
use dioxus_primitives::dioxus_attributes::attributes;
use dioxus_primitives::merge_attributes;

#[css_module("/src/kit/button/style.css")]
struct Styles;

#[derive(Copy, Clone, PartialEq, Default)]
#[non_exhaustive]
pub enum ButtonVariant {
    Primary,
    #[default]
    Secondary,
    Destructive,
    Outline,
    Ghost,
    Link,
}

impl ButtonVariant {
    pub fn class(&self) -> &'static str {
        match self {
            ButtonVariant::Primary => "primary",
            ButtonVariant::Secondary => "secondary",
            ButtonVariant::Destructive => "destructive",
            ButtonVariant::Outline => "outline",
            ButtonVariant::Ghost => "ghost",
            ButtonVariant::Link => "link",
        }
    }
}

#[derive(Copy, Clone, PartialEq, Default)]
#[non_exhaustive]
pub enum ButtonSize {
    Xs,
    Sm,
    #[default]
    Default,
    Lg,
    Icon,
    IconXs,
    IconSm,
    IconLg,
}

impl ButtonSize {
    pub fn class(&self) -> &'static str {
        match self {
            ButtonSize::Xs => "xs",
            ButtonSize::Sm => "sm",
            ButtonSize::Default => "default",
            ButtonSize::Lg => "lg",
            ButtonSize::Icon => "icon",
            ButtonSize::IconXs => "icon-xs",
            ButtonSize::IconSm => "icon-sm",
            ButtonSize::IconLg => "icon-lg",
        }
    }
}

#[component]
pub fn Button(
    #[props(default = ButtonVariant::Secondary)] variant: ButtonVariant,
    #[props(default)] size: ButtonSize,
    #[props(extends=GlobalAttributes)]
    #[props(extends=button)]
    attributes: Vec<Attribute>,
    onclick: Option<EventHandler<MouseEvent>>,
    onmousedown: Option<EventHandler<MouseEvent>>,
    onmouseup: Option<EventHandler<MouseEvent>>,
    onkeydown: Option<EventHandler<KeyboardEvent>>,
    onmounted: Option<EventHandler<MountedEvent>>,
    children: Element,
) -> Element {
    let base = attributes!(button {
        class: Styles::dx_button,
        "data-erpx-component": "button",
        "data-style": variant.class(),
        "data-tone": variant.class(),
        "data-size": size.class(),
    });
    let merged = merge_attributes(vec![base, attributes]);

    if let Some(onmounted) = onmounted {
        rsx! {
            button {
                onclick: move |event| if let Some(f) = &onclick { f.call(event); },
                onmousedown: move |event| if let Some(f) = &onmousedown { f.call(event); },
                onmouseup: move |event| if let Some(f) = &onmouseup { f.call(event); },
                onkeydown: move |event| if let Some(f) = &onkeydown { f.call(event); },
                onmounted: move |event| onmounted.call(event),
                ..merged,
                {children}
            }
        }
    } else {
        rsx! {
            button {
                onclick: move |event| if let Some(f) = &onclick { f.call(event); },
                onmousedown: move |event| if let Some(f) = &onmousedown { f.call(event); },
                onmouseup: move |event| if let Some(f) = &onmouseup { f.call(event); },
                onkeydown: move |event| if let Some(f) = &onkeydown { f.call(event); },
                ..merged,
                {children}
            }
        }
    }
}
