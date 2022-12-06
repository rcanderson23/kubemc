use tabled::{Style, Table, Tabled};

#[derive(Tabled, Clone, Debug)]
#[tabled(rename_all = "UPPERCASE")]
pub struct Output {
    pub cluster: String,
    pub namespace: String,
    pub name: String,
    pub status: String,
    pub ready: String,
    pub age: String,
}

pub(crate) fn create_table(outputs: Vec<Output>) {
    let mut table = Table::new(&outputs);
    table.with(Style::blank());
    println!("{}", table)
}
