# co3-types Architecture

## 1. Representation Family

Representation family categorization is done through the following trait:

```rs
trait ReprFamily {
    type Kind;
}
```

where `ReprFamily::Kind` is assigned one of the categories below through a marker of the same name:

1. **`Stable<Robust>`** (marker type)
- Types with stable C layout and no trap representations (e.g. `u32`).
- Usually map directly to themselves in ABI (no conversion necessary).

2. **`Stable<NonRobust>`** (marker type)
- Types that can be safely transmuted into a single chosen target type.
- IR/ABI mapping and value conversion continue through the target type.

3. **`Unstable`** (marker type)
- Fallback for types that don't belong to any of the previous IR type families.
- Conversion of references/slices piggybacks on the referent and incurs cloning.

4. **`Opaque`** (marker type)
- Types passed across FFI as opaque pointers, derived from a `Box`ed value.
- Consuming side SHOULD NOT rely on the layout of the referent or access its value.

### 1.1 Composite Types

The tables below specifies how composite types derive `ReprFamily::Kind`:

#### `[R]`

| R::Kind | Self::Kind |
| --- | --- |
| `Robust` | `Robust` |
| `Opaque` | `[Opaque]` |
| `Transmuted` | `Transmuted` |
| `Cloned` | `[R::Kind]` |

#### `&R`

| R::Kind | Self::Kind (R: Sized) | Self::Kind (R: !Sized) |
| --- | --- | --- |
| `Robust` | `Transmuted` | `&Robust` |
| `Opaque` | `Transmuted` | `&Opaque` |
| `Transmuted` | `Transmuted` | `&Transmuted` |
| `Cloned` | `&R::Kind`[1] | `&R::Kind`[1] |

#### `&mut R`

| R::Kind | Self::Kind (R: Sized) | Self::Kind (R: !Sized) |
| --- | --- | --- |
| `Robust` | `Transmuted` | `&mut Robust` |
| `Opaque` | `Transmuted` | `&mut Opaque` |
| `Transmuted` | `Transmuted` | `&mut Transmuted`[2] |
| `Cloned` | `&mut R::Kind`[1] | `&mut R::Kind`[1] |

#### `Box<R>`

| R::Kind | Self::Kind (R: Sized) | Self::Kind (R: !Sized) |
| --- | --- | --- |
| `Robust` | `Transmuted` | `Box<Robust>` |
| `Opaque` | `Transmuted` | `Box<Opaque>` |
| `Transmuted` | `Transmuted` | `Box<Transmuted>` |
| `Cloned` | `Box<R::Kind>` | `Box<R::Kind>` |

#### `Vec<R>`

| R::Kind | Self::Kind |
| --- | --- |
| `Robust` | `Vec<Robust>` |
| `Opaque` | `Vec<Box<Opaque>>` |
| `Transmuted` | `Vec<Transmuted>` |
| `Cloned` | `Vec<R::Kind>` |

#### `[R; N]`

| R::Kind | Self::Kind |
| --- | --- |
| `Robust` | `Robust` |
| `Opaque` | `[Opaque; N]` |
| `Transmuted` | `Transmuted` |
| `Cloned` | `[R::Kind; N]` |

#### `Option<R>`

| R::Kind | \<R as NicheFamily\>::Kind | Self::Kind |
| --- | --- | --- |
| `Robust` | `-` | `Option<WithoutNiche>` |
| `Opaque` | `-` | `Option<WithCustomNiche>` |
| `Transmuted` | `WithoutNiche` | `Option<WithoutNiche>` |
| `Transmuted` | `WithStableNiche` | `Transmuted` |
| `Transmuted` | `WithCustomNiche` | `Option<WithCustomNiche>` |
| `Cloned` | `WithoutNiche` | `Option<WithoutNiche>` |
| `Cloned` | `WithCustomNiche` | `Option<WithCustomNiche>` |

- `[1]` - Conditional on `#[soft]`
- `[2]` - For non-robust `R`, `Encode` path of `&mut R` is conditional on `#[soft]` or `unsafe-optimizations`

## 2. Niche Family

Niche family categorization is done through the following trait:

```rs
trait NicheFamily {
    type Kind;
}
```

where `NicheFamily::Kind` is assigned one of the categories below through a marker of the same name:

1. **`WithStableNiche`** (marker type)
- Type has a compiler-guaranteed niche value (refer to [doc](https://doc.rust-lang.org/std/option/#representation)).

2. **`WithCustomNiche`** (marker type)
- Type has a `crate`-defined sentinel niche value and `Option<T>` is encoded as `T::CType`.

3. **`WithoutNiche`** (marker type)
- Type has no niche value and `Option<T>` must be encoded as a 2-tuple with a discriminant.

### 2.1 Composite Types

The tables below specifies how composite types derive `NicheFamily::Kind`:

| Self | `Self::Kind` |
| --- | --- |
| `&R` | `WithStableNiche` |
| `&mut R` | `WithStableNiche` |
| `Box<R>` | `WithStableNiche` |
| `&[R]` | `WithCustomNiche` |
| `&mut [R]` | `WithCustomNiche` |
| `Box<[R]>` | `WithCustomNiche` |
| `Vec<R>` | `WithCustomNiche` |

#### `[R; N]`

| `R::NicheFamily::Kind` | `Self::Kind` |
| --- | --- |
| `WithStableNiche` | `WithCustomNiche` |
| `WithCustomNiche` | `WithCustomNiche` |
| `WithoutNiche` | `WithoutNiche` |

#### `Option<R>`

| `R::NicheFamily::Kind` | `Self::Kind` |
| --- | --- |
| `WithoutNiche` | `WithCustomNiche` |
| `WithStableNiche` | `WithoutNiche` |
| `WithCustomNiche` | `WithCustomNiche` |

## 3. Mutability Family

Mutability family categorization is done through the following trait:

```rs
trait MutabilityFamily {
    type Kind;
}
```

where `MutabilityFamily::Kind` is assigned one of the categories below through a marker of the same name:

1. **`Interior`** (marker type)
- Types whose reachable state may be mutated through shared access.
- `UnsafeCell<T>` and wrappers that expose interior mutability belong to this family.

2. **`Exclusive`** (marker type)
- Types whose reachable state requires exclusive access to mutate.

### 3.1 Composite Types

Composite types derive `MutabilityFamily::Kind` structurally:

| Field kinds | Self::Kind |
| --- | --- |
| Any `Interior` field | `Interior` |
| All `Exclusive` fields | `Exclusive` |

Transparent ownership/reference wrappers propagate the wrapped type's mutability family.
Raw pointers and `NonNull<T>` are classified as `Exclusive` regardless of `T`, because mutating their pointee requires an unsafe dereference rather than shared Rust access to the pointer value.
