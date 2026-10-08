//! `triage` / `callout` / `hero` section renderers (MINI_ORK_IDE_SPEC=2).
//! They draw their own chrome so the generic card+title header in
//! `MiniOrkPage::render_section` doesn't wrap them.

use gpui::{AnyElement, Context, SharedString, px};
use serde_json::Value;
use ui::prelude::*;

use crate::{
    components::card,
    page::{arr, str_of, MiniOrkPage},
    palette::{self as p, c, named},
    select::Look,
};

impl MiniOrkPage {
    pub(crate) fn render_triage(&self, index: usize, s: &Value, cx: &mut Context<Self>) -> AnyElement {
        let tone = str_of(s, "tone");
        let text = str_of(s, "text");
        let icon = str_of(s, "icon");
        let detail = str_of(s, "detail");
        let counts = arr(s, "counts");
        let actions = arr(s, "actions");
        let menu = arr(s, "menu");
        let tone_colour = named(&tone);

        let accent = div()
            .absolute()
            .left_0()
            .top(px(p::RADIUS_CARD))
            .bottom(px(p::RADIUS_CARD))
            .w(px(3.))
            .rounded_full()
            .bg(tone_colour);

        let row1 = h_flex()
            .gap(px(10.))
            .items_center()
            .flex_wrap()
            .child(
                div()
                    .flex_none()
                    .text_size(px(14.))
                    .text_color(tone_colour)
                    .child(SharedString::from(icon)),
            )
            .child(
                self.texts.text(
                    format!("s{index}-triage-text"),
                    &text,
                    Look::new(13., c(p::TEXT)).bold(true),
                    cx,
                ),
            )
            .child(
                h_flex()
                    .flex_1()
                    .min_w(px(0.))
                    .justify_end()
                    .gap(px(6.))
                    .flex_wrap()
                    .children(counts.iter().enumerate().map(|(i, item)| {
                        let colour = named(&str_of(item, "c"));
                        let tint = colour.opacity(0.12);
                        h_flex()
                            .flex_none()
                            .id(SharedString::from(format!("s{index}-count{i}")))
                            .h(px(18.))
                            .px(px(8.))
                            .items_center()
                            .rounded_full()
                            .bg(tint)
                            .text_size(px(p::BADGE))
                            .text_color(colour)
                            .child(str_of(item, "t"))
                            .into_any_element()
                    })),
            );

        let mut body = v_flex().relative().gap(px(8.)).child(row1);

        if !detail.is_empty() {
            body = body.child(self.texts.text(
                format!("s{index}-triage-detail"),
                &detail,
                Look::new(11.5, c(p::MUTED)),
                cx,
            ));
        }

        if !actions.is_empty() || !menu.is_empty() {
            let mut btn_row = h_flex().gap(px(6.)).flex_wrap().items_center();
            for (i, b) in actions.iter().enumerate() {
                btn_row = btn_row.child(
                    self.button(SharedString::from(format!("s{index}-triage-a{i}")), b, 24., 12., cx),
                );
            }
            if !menu.is_empty() {
                if !actions.is_empty() {
                    btn_row = btn_row.child(div().w(px(1.)).h(px(16.)).bg(c(p::BORDER2)));
                }
                for (i, b) in menu.iter().enumerate() {
                    let mut ghost = b.clone();
                    if let Some(map) = ghost.as_object_mut() {
                        map.insert("kind".to_string(), Value::String("ghost".to_string()));
                    }
                    btn_row = btn_row.child(
                        self.button(SharedString::from(format!("s{index}-triage-m{i}")), &ghost, 24., 12., cx),
                    );
                }
            }
            body = body.child(btn_row);
        }

        card()
            .relative()
            .flex_1()
            .min_w_0()
            .p(px(14.))
            .child(accent)
            .child(body)
            .into_any_element()
    }

    pub(crate) fn render_callout(&self, index: usize, s: &Value, cx: &mut Context<Self>) -> AnyElement {
        let tone = str_of(s, "tone");
        let title = str_of(s, "title");
        let body_md = str_of(s, "text_md");
        let actions = arr(s, "actions");
        let tone_colour = named(&tone);

        let head = h_flex()
            .gap(px(8.))
            .items_center()
            .child(
                div()
                    .flex_none()
                    .text_size(px(13.))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(tone_colour)
                    .child(SharedString::from("?")),
            )
            .when(!title.is_empty(), |this| {
                this.child(self.texts.text(
                    format!("s{index}-callout-title"),
                    &title,
                    Look::new(13., c(p::TEXT)).bold(true),
                    cx,
                ))
            });

        let mut body = v_flex().gap(px(10.)).child(head);

        if !body_md.is_empty() {
            body = body.child(self.texts.markdown(
                format!("s{index}-callout-md"),
                &body_md,
                Look::new(12.5, c(p::BODY)),
                cx,
            ));
        }

        if !actions.is_empty() {
            body = body.child(
                h_flex()
                    .justify_end()
                    .gap(px(6.))
                    .flex_wrap()
                    .children(actions.iter().enumerate().map(|(i, b)| {
                        self.button(SharedString::from(format!("s{index}-callout-a{i}")), b, 24., 12., cx)
                    })),
            );
        }

        card()
            .p(px(12.))
            .flex_1()
            .min_w_0()
            .border_color(tone_colour.opacity(0.45))
            .bg(tone_colour.opacity(0.06))
            .child(body)
            .into_any_element()
    }

    pub(crate) fn render_hero(&self, index: usize, s: &Value, cx: &mut Context<Self>) -> AnyElement {
        let title = str_of(s, "title");
        let goal = str_of(s, "goal");
        let pill = s.get("pill").filter(|v| !v.is_null());
        let meta = arr(s, "meta");
        let criteria = arr(s, "criteria");

        let mut content = v_flex().gap(px(10.));

        if !title.is_empty() || pill.is_some() {
            let mut title_row = h_flex().gap(px(10.)).items_center().flex_wrap();
            if !title.is_empty() {
                title_row = title_row.child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .text_size(px(11.))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(c(p::MUTED))
                        .child(SharedString::from(title.to_uppercase())),
                );
            }
            if let Some(pill_value) = pill {
                // Inlined (instead of components::state_pill) because the pill's
                // colour comes from the page's `c`, not from a state word, so
                // state_pill's palette::state_mark lookup would ignore it.
                let t = str_of(pill_value, "t");
                let colour = named(&str_of(pill_value, "c"));
                if !t.is_empty() {
                    title_row = title_row.child(
                        h_flex()
                            .flex_none()
                            .h(px(18.))
                            .px(px(8.))
                            .items_center()
                            .rounded_full()
                            .bg(colour.opacity(0.15))
                            .text_size(px(p::PILL))
                            .text_color(colour)
                            .child(SharedString::from(t.to_uppercase())),
                    );
                }
            }
            content = content.child(title_row);
        }

        if !goal.is_empty() {
            content = content.child(self.texts.markdown(
                format!("s{index}-hero-goal"),
                &goal,
                Look::new(14., c(p::TEXT)),
                cx,
            ));
        }

        if !criteria.is_empty() {
            content = content.child(
                v_flex()
                    .gap(px(4.))
                    .mt(px(2.))
                    .children(criteria.iter().enumerate().map(|(i, crit)| {
                        let text = crit.as_str().unwrap_or_default().to_string();
                        h_flex()
                            .gap(px(8.))
                            .items_center()
                            .child(
                                div()
                                    .flex_none()
                                    .text_size(px(12.5))
                                    .text_color(c(p::MUTED))
                                    .child(SharedString::from("○")),
                            )
                            .child(self.texts.text(
                                format!("s{index}-hero-c{i}"),
                                &text,
                                Look::new(12.5, c(p::BODY)),
                                cx,
                            ))
                    })),
            );
        }

        if !meta.is_empty() {
            let mut row = h_flex().gap(px(6.)).items_center().flex_wrap();
            for (i, item) in meta.iter().enumerate() {
                let colour = named(&str_of(item, "c"));
                let mono_flag = item.get("mono").and_then(Value::as_bool).unwrap_or(false);
                row = row.child(self.texts.text(
                    format!("s{index}-hero-m{i}"),
                    str_of(item, "t"),
                    Look::new(11., colour).mono(mono_flag),
                    cx,
                ));
                if i + 1 < meta.len() {
                    row = row.child(
                        div()
                            .text_size(px(11.))
                            .text_color(c(p::DIM))
                            .child(SharedString::from("·")),
                    );
                }
            }
            content = content.child(row);
        }

        card().p(px(14.)).flex_1().min_w_0().child(content).into_any_element()
    }
}
