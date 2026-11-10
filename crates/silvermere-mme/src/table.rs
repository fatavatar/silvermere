//! The rows of an MME database, as the export produces them and the Jet writer stores them.

use std::collections::BTreeMap;

/// One field's value. Columns are typed by the template database; a value is converted to
/// its column's type when written (a `Long` into a `Double` column, say).
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// Access `Long Integer` (also written into `Byte`/`Integer` columns when it fits).
    Long(i32),
    /// Access `Double`.
    Double(f64),
    /// Access `Text` (at most the column's length) or `Memo`.
    Text(String),
    /// Access `Binary` (at most the column's length) or `OLE Object`.
    Bytes(Vec<u8>),
    /// No value.
    Null,
}

impl From<i32> for Value {
    fn from(v: i32) -> Self {
        Value::Long(v)
    }
}

impl From<f64> for Value {
    fn from(v: f64) -> Self {
        Value::Double(v)
    }
}

impl From<&str> for Value {
    fn from(v: &str) -> Self {
        Value::Text(v.to_owned())
    }
}

impl From<String> for Value {
    fn from(v: String) -> Self {
        Value::Text(v)
    }
}

/// One row: column name (as the template spells it, e.g. `"Map Number"`, `"AttName-0"`) to
/// value. A column the row leaves out is written as 0 (numbers) or an empty string (text).
pub type Row = BTreeMap<String, Value>;

/// The rows of every table MME reads, in the order they are written. Rows need not be sorted:
/// the writer orders each table by its index.
#[derive(Debug, Clone, Default)]
pub struct Tables {
    pub info: Vec<Row>,
    pub classes: Vec<Row>,
    pub races: Vec<Row>,
    pub items: Vec<Row>,
    pub spells: Vec<Row>,
    pub monsters: Vec<Row>,
    pub shops: Vec<Row>,
    pub rooms: Vec<Row>,
    pub tbinfo: Vec<Row>,
    pub lairs: Vec<Row>,
}

impl Tables {
    /// Each table by its name in the database.
    pub fn by_name(&self) -> [(&'static str, &Vec<Row>); 10] {
        [
            ("Info", &self.info),
            ("Classes", &self.classes),
            ("Races", &self.races),
            ("Items", &self.items),
            ("Spells", &self.spells),
            ("Monsters", &self.monsters),
            ("Shops", &self.shops),
            ("Rooms", &self.rooms),
            ("TBInfo", &self.tbinfo),
            ("Lairs", &self.lairs),
        ]
    }
}
