// SPDX-License-Identifier: GPL-3.0-or-later

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
use tracing::{
    Event, Subscriber,
    field::{Field, Visit},
    span::{Attributes, Id},
};
use tracing_subscriber::{
    EnvFilter, Layer,
    layer::{Context, SubscriberExt},
    registry::LookupSpan,
};

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Text(String),
    Unsigned(u64),
    Signed(i64),
    Bool(bool),
}
#[derive(Clone, Debug, Default)]
pub struct Fields(pub BTreeMap<String, Value>);
impl Fields {
    pub fn text(&self, name: &str) -> Option<&str> {
        match self.0.get(name) {
            Some(Value::Text(s)) => Some(s),
            _ => None,
        }
    }
}
impl Visit for Fields {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.0
            .insert(field.name().into(), Value::Text(value.into()));
    }
    fn record_u64(&mut self, field: &Field, value: u64) {
        self.0.insert(field.name().into(), Value::Unsigned(value));
    }
    fn record_i64(&mut self, field: &Field, value: i64) {
        self.0.insert(field.name().into(), Value::Signed(value));
    }
    fn record_bool(&mut self, field: &Field, value: bool) {
        self.0.insert(field.name().into(), Value::Bool(value));
    }
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.0
            .insert(field.name().into(), Value::Text(format!("{value:?}")));
    }
}
#[derive(Clone, Debug)]
pub struct SpanRecord {
    pub name: &'static str,
    pub fields: Fields,
}
#[derive(Clone, Debug)]
pub struct Record {
    pub target: &'static str,
    pub level: tracing::Level,
    pub fields: Fields,
    pub scope: Vec<SpanRecord>,
}
#[derive(Clone, Default)]
pub struct Capture(Arc<Mutex<Vec<Record>>>);
impl Capture {
    pub fn subscriber(&self, filter: &str) -> impl Subscriber + Send + Sync + use<> {
        tracing_subscriber::registry()
            .with(
                EnvFilter::builder()
                    .with_regex(false)
                    .parse(filter)
                    .unwrap(),
            )
            .with(self.clone())
    }
    pub fn events(&self) -> Vec<Record> {
        self.0.lock().unwrap().clone()
    }
}
impl<S: Subscriber + for<'a> LookupSpan<'a>> Layer<S> for Capture {
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        let mut fields = Fields::default();
        attrs.record(&mut fields);
        ctx.span(id).unwrap().extensions_mut().insert(SpanRecord {
            name: attrs.metadata().name(),
            fields,
        });
    }
    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        let mut fields = Fields::default();
        event.record(&mut fields);
        let scope = ctx
            .event_scope(event)
            .map(|scope| {
                scope
                    .from_root()
                    .map(|s| s.extensions().get::<SpanRecord>().unwrap().clone())
                    .collect()
            })
            .unwrap_or_default();
        self.0.lock().unwrap().push(Record {
            target: event.metadata().target(),
            level: *event.metadata().level(),
            fields,
            scope,
        });
    }
}
