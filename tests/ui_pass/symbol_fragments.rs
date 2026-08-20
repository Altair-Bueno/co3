use co3::ffi;

mod odbc {
    pub struct SQLCHAR;
    pub struct SQLWCHAR;
}

fn exported() {}

ffi! {
    #![unsafe(export("C"))]

    #![symbol_fragments {
        odbc::SQLCHAR = "A",
        odbc::SQLWCHAR = "W"
    }]

    fn exported();
}

ffi! {
    #![unsafe(extern("C"))]

    #![symbol_fragments {
        odbc::SQLCHAR = "A",
        odbc::SQLWCHAR = "W"
    }]

    fn imported();

    #[symbol_name = "imported_{T}"]
    pub fn imported_static<T>()
    where
        use<T> @ (<odbc::SQLCHAR> | <odbc::SQLWCHAR>);
}

fn main() {}
