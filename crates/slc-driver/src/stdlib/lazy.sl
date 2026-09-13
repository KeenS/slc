// `lazy`: a one-item menu is a by-name thunk.

mod lazy {
    pub menu Lazy<T> {
        force: T,
    }
}
