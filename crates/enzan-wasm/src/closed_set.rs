//! The macro that declares a closed set.

/// Declares an enum and a list of all its variants, in the order declared.
///
/// A variant added to the enum is in `ALL` without a second list to keep in
/// step with it. It makes `ALL` only: a name is taken from serde, where the
/// names are already written once.
macro_rules! closed_set {
    (
        $(#[$m:meta])*
        $vis:vis enum $name:ident {
            $( $(#[$vm:meta])* $variant:ident ),+ $(,)?
        }
    ) => {
        $(#[$m])*
        $vis enum $name {
            $( $(#[$vm])* $variant ),+
        }

        impl $name {
            /// Every variant, in the order declared.
            pub const ALL: &'static [Self] = &[ $( Self::$variant ),+ ];
        }
    };
}
