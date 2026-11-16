// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! B-345: a price of a currency KassenSturz's tables don't list whose
//! numerator is i128::MIN. Synthetic data only.

use kassensturz_import::{Kmy, Limits, import};

const FILE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE KMYMONEY-FILE>
<KMYMONEY-FILE>
  <FILEINFO>
    <VERSION id="1"/>
  </FILEINFO>
  <ACCOUNTS>
    <ACCOUNT id="AStd::Asset" parentaccount="" type="9" name="Assets" currency="EUR"/>
    <ACCOUNT id="A000003" parentaccount="AStd::Asset" type="3" name="Old cash" currency="ROL"/>
  </ACCOUNTS>
  <TRANSACTIONS/>
  <CURRENCIES>
    <CURRENCY id="EUR" name="Euro" symbol="EUR" type="3" rounding-method="0" saf="100" pp="4" scf="100"/>
    <CURRENCY id="ROL" name="Old" symbol="ROL" type="3" rounding-method="0" saf="100" pp="4" scf="100"/>
  </CURRENCIES>
  <PRICES>
    <PRICEPAIR from="ROL" to="RON">
      <PRICE date="2005-06-30" price="-170141183460469231731687303715884105728/1" source="KMyMoney"/>
    </PRICEPAIR>
  </PRICES>
</KMYMONEY-FILE>
"#;

#[test]
fn a_price_with_the_smallest_numerator_is_no_panic() {
    let result = std::panic::catch_unwind(|| import(&Kmy, FILE.as_bytes(), &Limits::default()));
    assert!(result.is_ok(), "the importer panicked");
}
