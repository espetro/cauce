// rust-i18n embeds `locales/` at macro-expansion time but the proc macro
// does not track the files, so an `en.yaml` edit alone would not rebuild
// the crate. Register the directory here.
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

fn main() {
    println!("cargo:rerun-if-changed=locales");
}
