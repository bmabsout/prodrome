//! An event of a declared schema, and an entity's registers under one: the
//! `Schema` trait family implemented by interpreting the declaration.

use crate::event::{Actor, Hash};
use crate::fold::{grows, Frontier, Product, Register};
use crate::literal::{Call, Datetime, ProdromeError, Value};
use crate::payload::required;
use crate::registers::Stamp;
use crate::schema::Schema;

use super::language::Type;
use super::order::{Climbing, Valued};
use super::{Declaration, Slot};

/// A field's value, of one of the declared types.
///
/// `Ord` only so a list can be read as a set; a register's order is never
/// this one but its declared order ([`Valued`]).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Datum {
    Text(String),
    Integer(i64),
    Instant(Datetime),
    Reference(Hash),
    Alternative(String),
    List(Vec<Datum>),
}

impl Datum {
    /// `value` as a value of the type `ty`, through every smart constructor.
    fn parse(ty: &Type, value: &Value, context: &str) -> Result<Datum, ProdromeError> {
        let refused = || {
            ProdromeError::invalid(format!(
                "{context} must be a value of {ty:?}, got {value:?}"
            ))
        };
        match (ty, value) {
            (Type::Text, Value::Str(text)) => Ok(Datum::Text(text.clone())),
            (Type::Integer, Value::Int(int)) => {
                int.as_i64().map(Datum::Integer).ok_or_else(refused)
            }
            (Type::Instant, Value::Datetime(at)) => Ok(Datum::Instant(*at)),
            (Type::Reference, Value::Str(text)) => Ok(Datum::Reference(Hash::new(text.clone())?)),
            (Type::Enum(alternatives), Value::Str(text)) if alternatives.contains(text) => {
                Ok(Datum::Alternative(text.clone()))
            }
            (Type::List(item), Value::Tuple(items)) => items
                .iter()
                .map(|value| Datum::parse(item, value, context))
                .collect::<Result<_, _>>()
                .map(Datum::List),
            _ => Err(refused()),
        }
    }

    fn to_value(&self) -> Value {
        match self {
            Datum::Text(text) | Datum::Alternative(text) => Value::str(text.as_str()),
            Datum::Integer(int) => Value::int(*int),
            Datum::Instant(at) => Value::Datetime(*at),
            Datum::Reference(name) => Value::str(name.as_str()),
            Datum::List(items) => Value::Tuple(items.iter().map(Datum::to_value).collect()),
        }
    }
}

/// What a declared event is about: its key field's text.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Key(String);

impl AsRef<str> for Key {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// An event of a declared schema: one of its constructors, every field a
/// value of its declared type. It holds its declaration (one shared value),
/// so it routes itself; it never prints it, so no object carries a schema.
#[derive(Debug, Clone)]
pub struct Declared {
    schema: Declaration,
    event: usize,
    key: Key,
    at: Datetime,
    actor: Actor,
    data: Vec<Datum>,
}

impl PartialEq for Declared {
    fn eq(&self, other: &Self) -> bool {
        self.schema == other.schema && self.event == other.event && self.data == other.data
    }
}

impl Declared {
    /// The declaration this event is of.
    #[must_use]
    pub fn schema(&self) -> &Declaration {
        &self.schema
    }

    /// Its constructor's name.
    #[must_use]
    pub fn constructor(&self) -> &str {
        &self.schema.shape(self.event).name
    }

    /// Every field's value, in declared order.
    #[must_use]
    pub fn data(&self) -> &[Datum] {
        &self.data
    }

    /// The value this event writes to the register `slot`, none where it
    /// writes none.
    #[must_use]
    pub fn written(&self, slot: Slot) -> Option<&Datum> {
        self.schema
            .shape(self.event)
            .writes
            .iter()
            .find(|(written, _)| *written == slot)
            .map(|(_, field)| &self.data[*field])
    }

    /// The value written to `slot` under that register's order.
    fn valued(&self, slot: Slot) -> Valued<'_> {
        let datum = self
            .written(slot)
            .expect("a write joins only the registers it writes");
        self.schema.value(slot, datum)
    }
}

impl Schema for Declared {
    type Vocabulary = Declaration;
    type Key = Key;
    type Register = Slot;
    type Registers<'a> = Registers<'a>;

    fn registers(schema: &Declaration) -> Vec<Slot> {
        schema.registers().collect()
    }

    fn to_value(&self) -> Value {
        let shape = self.schema.shape(self.event);
        Value::call(
            shape.name.as_str(),
            shape
                .names
                .iter()
                .cloned()
                .zip(self.data.iter().map(Datum::to_value))
                .collect(),
        )
    }

    fn from_value(schema: &Declaration, value: &Value) -> Result<Declared, ProdromeError> {
        let call: &Call = value
            .as_call()
            .ok_or_else(|| ProdromeError::invalid("a declared event is a constructor call"))?;
        let event = schema.event(&call.name).ok_or_else(|| {
            ProdromeError::invalid(format!("{} is not an event of this schema", call.name))
        })?;
        let shape = schema.shape(event);
        let data = shape
            .names
            .iter()
            .zip(&shape.types)
            .map(|(name, ty)| {
                Datum::parse(ty, required(call, name)?, &format!("{}.{name}", call.name))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let (Datum::Text(key), Datum::Instant(at), Datum::Text(actor)) =
            (&data[shape.key], &data[shape.at], &data[shape.actor])
        else {
            unreachable!("the key and stamp laws type these fields");
        };
        Ok(Declared {
            key: Key(key.clone()),
            at: *at,
            actor: Actor::new(actor.as_str())?,
            schema: schema.clone(),
            event,
            data,
        })
    }

    fn key(&self) -> &Key {
        &self.key
    }

    fn at(&self) -> Datetime {
        self.at
    }

    fn actor(&self) -> &Actor {
        &self.actor
    }

    fn writes(&self) -> impl Iterator<Item = Slot> {
        self.schema
            .shape(self.event)
            .writes
            .iter()
            .map(|(slot, _)| *slot)
    }
}

/// An entity's registers under a declared schema: a product of one
/// [`Frontier`] per register, each read under its declared order. It learns
/// the declaration from the first write it joins; before one it holds
/// nothing, which is every register unwritten.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Registers<'a> {
    frontiers: Vec<Frontier<'a, Declared>>,
    unwritten: Frontier<'a, Declared>,
}

impl Registers<'_> {
    fn hold(&mut self, slot: Slot) {
        if self.frontiers.len() <= slot.index() {
            self.frontiers
                .resize_with(slot.index() + 1, Frontier::default);
        }
    }
}

impl<'a> Product<'a> for Registers<'a> {
    type Schema = Declared;

    fn registers(&self) -> Vec<Slot> {
        (0..self.frontiers.len()).map(Slot).collect()
    }

    fn join(&mut self, stamp: &'a Stamp<Declared>) {
        if let Some(last) = stamp.event.schema.registers().last() {
            self.hold(last);
        }
        for slot in stamp.event.writes() {
            self.frontiers[slot.index()].join(stamp);
        }
    }

    fn frontier(&self, slot: Slot) -> &Frontier<'a, Declared> {
        self.frontiers.get(slot.index()).unwrap_or(&self.unwritten)
    }

    fn frontier_mut(&mut self, slot: Slot) -> &mut Frontier<'a, Declared> {
        self.hold(slot);
        &mut self.frontiers[slot.index()]
    }

    /// The frontier's maximal writes under the register's declared order:
    /// the completion every register is read by.
    fn reading(&self, slot: Slot) -> Vec<&'a Stamp<Declared>> {
        self.frontier(slot)
            .read(|stamp: &'a Stamp<Declared>| stamp.event.valued(slot))
    }

    /// Where `event` writes a register declared inflationary, its value must
    /// stand at or above that register's reading: [`grows`], the refusal
    /// every inflationary register is held to.
    fn grows(&self, event: &Declared) -> Result<(), ProdromeError> {
        for slot in event.writes() {
            if !event.schema.inflationary(slot) {
                continue;
            }
            let held: Vec<Climbing<'_>> = self
                .reading(slot)
                .into_iter()
                .map(|stamp| Climbing(stamp.event.valued(slot)))
                .collect();
            grows(&held, &Climbing(event.valued(slot))).map_err(|why| {
                ProdromeError::invalid(format!("{}: {why}", event.schema.register_name(slot)))
            })?;
        }
        Ok(())
    }
}
