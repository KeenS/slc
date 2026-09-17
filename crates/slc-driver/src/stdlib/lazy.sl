// `lazy`: a one-item menu is a by-name thunk.

mod lazy {
    pub menu Lazy<*T, E> / {..E} {
        force: T,
    }

    pub fn of_delayed<-T, E>(computation: Delayed<T, ..E>) -> Lazy<T, ..E> {
        mu Lazy<T, ..E> {
            force <= {
                let+ value = computation;
                <value | force>
            },
        }
    }

    pub fn to_delayed<-T, E>(computation: Lazy<T, ..E>) -> Delayed<T, ..E> {
        let- pending = computation.force;
        pending
    }
}
