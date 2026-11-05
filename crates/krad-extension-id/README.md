# krad-extension-id

The validated extension id of [Krad ADE](https://github.com/joordih/krad-ade).

An id is 1 to 64 lowercase letters, digits, `-`, `_` and `.`, never starting with `.` and never holding `..`. That keeps every id a single safe path segment, so an extension can never name a folder outside its own.

```rust
use krad_extension_id::ExtensionId;

let id: ExtensionId = "docker".parse()?;
assert!("..".parse::<ExtensionId>().is_err());
# Ok::<(), krad_extension_id::IdError>(())
```

Licensed under MIT or Apache-2.0, at your option.
