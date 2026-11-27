// Host reason (ARC-03): focused contracts for the source-level API checker.
import assert from 'node:assert/strict';
import test from 'node:test';
import { publicSurface, rustTokens } from '../lib/rust-surface.mts';

const facade = `
/// Parse a document.
pub fn parse<'a>(source: &'a str) -> Result<Document, Diagnostic> {
    let tricky = r##"{ /* not a comment */ }"##;
    private_parse(source)
}
pub struct Document {
    pub title: String,
    cache: Vec<u8>,
}
fn private_parse(source: &str) { /* not an export */ }
`;

test('a changed or removed public signature differs from its original surface', () => {
  const before = publicSurface(facade);
  assert.equal(before.find(item => item.name === 'parse')!.declaration, "pub fn parse < ' a > ( source : & ' a str ) - > Result < Document , Diagnostic >");
  assert.notDeepEqual(publicSurface(facade.replace("&'a str", "&'a [u8]")), before);
  assert.notDeepEqual(publicSurface(facade.replace('pub fn parse', 'fn parse')), before);
});

test('formatting, documentation, function bodies and private field types do not become the API', () => {
  const before = publicSurface(facade);
  const after = facade.replace('/// Parse a document.', '/** Changed documentation. */')
    .replace('pub fn parse', 'pub\nfn /* whitespace */ parse')
    .replace('private_parse(source)', 'panic!("different body with } and ;")')
    .replace('cache: Vec<u8>', 'secret_cache: [u64; 32]');
  assert.deepEqual(publicSurface(after), before);
  assert.equal(JSON.stringify(before).includes('cache'), false);
  assert.deepEqual(before.find(item => item.name === 'Document')!.publicFields, ['pub title : String']);
});

test('explicit reexports keep their public aliases and targets, independent of grouping or order', () => {
  const grouped = publicSurface('pub use crate::api::{Document, parse as read};');
  const separate = publicSurface('pub use crate::api::parse as read; pub use crate::api::Document;');
  assert.deepEqual(grouped, separate);
  assert.notDeepEqual(grouped, publicSurface('pub use crate::other::{Document, parse as read};'));
  assert.deepEqual(publicSurface('pub use crate::api::{self, Document};').map(item => item.name), ['Document', 'api']);
});

test('supplementary declarations select only the producer-approved type', () => {
  const source = `${facade}\npub trait Unrelated { fn hidden(); }\nimpl UnrelatedType { pub fn helper() {} }`;
  assert.deepEqual(publicSurface(source, { select: ['Document'] }), publicSurface(facade, { select: ['Document'] }));
  assert.throws(() => publicSurface(source, { select: ['RemovedType'] }), /Registered public item RemovedType is missing/);
});

test('unsupported or ambiguous public forms fail instead of disappearing from a snapshot', () => {
  for (const source of ['pub use crate::api::*;', 'generate_api!();', 'pub mod nested { pub fn hidden() {} }', 'extern "C" { pub fn foreign(); }', 'pub fn duplicate() {} pub fn duplicate() {}']) {
    assert.throws(() => publicSurface(source), /Wildcard|Unsupported|macros|Ambiguous|extern/);
  }
});

test('external module declarations enter an inventory only by explicit opt-in', () => {
  const source = 'pub mod document; pub mod registry; pub use crate::style::Leaf as StyleLeaf; fn private_cache() {}';
  assert.throws(() => publicSurface(source), /Unsupported public Rust declaration: mod/);
  const inventory = publicSurface(source, { moduleDeclarations: true });
  assert.deepEqual(inventory.map(item => item.name), ['StyleLeaf', 'document', 'registry']);
  assert.deepEqual(inventory.find(item => item.name === 'document'), { name: 'document', kind: 'mod', declaration: 'pub mod document ;' });
  assert.equal(inventory.find(item => item.name === 'StyleLeaf')!.declaration, 'pub use crate : : style : : Leaf as StyleLeaf ;');
  assert.notDeepEqual(publicSurface(source.replace('pub mod document', 'mod document'), { moduleDeclarations: true }), inventory);
  assert.deepEqual(publicSurface(source.replace('private_cache() {}', 'private_cache() { panic!("changed"); }'), { moduleDeclarations: true }), inventory);
});

test('module inventory opt-in does not accept opaque module bodies, redirects or generated exports', () => {
  for (const source of ['pub mod inline { pub fn hidden() {} }', '#[path="other.rs"] pub mod redirected;', 'generate_exports!();']) {
    assert.throws(() => publicSurface(source, { moduleDeclarations: true }), /direct external module|Module-level macros/);
  }
});

test('tuple fields retain public positions without copying private types', () => {
  const source = 'pub struct Rgba(pub [f32; 4]); pub struct Opaque(private::State, pub u32);';
  const before = publicSurface(source);
  assert.deepEqual(before.find(item => item.name === 'Rgba')!.tupleFields, [{ index: 0, declaration: 'pub [ f32 ; 4 ]' }]);
  assert.deepEqual(before.find(item => item.name === 'Opaque')!.tupleFields, [{ index: 1, declaration: 'pub u32' }]);
  assert.equal(JSON.stringify(before).includes('State'), false, 'private type spelling is excluded');
  assert.deepEqual(publicSurface(source.replace('private::State', 'other::Cache')), before);
  assert.notDeepEqual(publicSurface(source.replace('private::State,', '')), before);
});

test('selected types retain trait capabilities, public methods and associated types without method bodies', () => {
  const source = `pub struct Options { pub width: f64 }
    impl Default for Options { fn default() -> Self { Self { width: 720.0 } } }
    impl Options { pub fn width(&self) -> f64 { self.width } fn private_cache(&self) {} }
    impl Iterator for Options { type Item = u32; fn next(&mut self) -> Option<u32> { None } }`;
  const before = publicSurface(source, { select: ['Options'] });
  assert.equal(before.filter(item => item.kind === 'impl').length, 3);
  assert.deepEqual(publicSurface(source.replace('width: 720.0', 'width: 800.0').replace('self.width }', '42.0 }'), { select: ['Options'] }), before);
  assert.notDeepEqual(publicSurface(source.replace('impl Default for Options', 'impl OtherTrait for Options'), { select: ['Options'] }), before);
  assert.notDeepEqual(publicSurface(source.replace('type Item = u32', 'type Item = u64'), { select: ['Options'] }), before);
  assert.notDeepEqual(publicSurface(source.replace('pub fn width', 'fn width'), { select: ['Options'] }), before);
  assert.equal(JSON.stringify(before).includes('private_cache'), false);
});

test('traits distinguish required/default methods and retain associated type bounds', () => {
  const source = 'pub trait Api: Send { type Item: Clone; fn read(&self) -> Self::Item; fn count(&self) -> usize { 1 } }';
  const before = publicSurface(source);
  assert.notDeepEqual(publicSurface(source.replace('Clone', 'Copy')), before);
  assert.notDeepEqual(publicSurface(source.replace('fn read(&self) -> Self::Item;', 'fn read(&self) -> Self::Item { todo!() }')), before);
  assert.deepEqual(publicSurface(source.replace('{ 1 }', '{ 99 }')), before);
});

test('literals and nested comments cannot inject public declarations or confuse body boundaries', () => {
  assert.deepEqual(publicSurface('/* outer /* pub fn fake() {} */ comment */ pub fn real() { let text = "pub fn fake() {}"; let ch = \'}\'; }').map(item => item.name), ['real']);
  assert.deepEqual(rustTokens('pub const ENTRY: &str = r#"/report-runtime.js"#;').slice(-2), ['r#"/report-runtime.js"#', ';']);
  assert.throws(() => rustTokens('/* unclosed'), /Unclosed/);
  assert.throws(() => publicSurface('pub fn broken() {'), /Unclosed/);
  assert.deepEqual(publicSurface('pub extern "C" fn count() -> u32 { pub struct Local; 1 }'), publicSurface('pub extern "C" fn count() -> u32 { 2 }'));
});
