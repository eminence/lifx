extern crate term;

use self::term::terminfo;
use std::borrow::Borrow;

pub struct TermMgr {
    vars: terminfo::parm::Variables,
    terminfo: terminfo::TermInfo
}

impl TermMgr {

    pub fn new() -> TermMgr {
        let terminfo = terminfo::TermInfo::from_env().unwrap();

        TermMgr {
            vars: terminfo::parm::Variables::new(),
            terminfo: terminfo
        }
        
    }

    pub fn clear(&mut self)  {
        let cap = self.terminfo.strings.get("clear").unwrap();
        let res = terminfo::parm::expand(cap, &[], &mut self.vars).unwrap();
        println!("{}", String::from_utf8_lossy(&res));


    }
}


