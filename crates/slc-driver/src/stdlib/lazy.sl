// `lazy`: a one-item menu is a by-name thunk.

pub menu Lazy<*T, E> / {..E} {
    force: T,
}

pub func of_delayed<-T, E>(computation: (-> T / {..E})) -> Lazy<T, ..E> {
    mu Lazy<T, ..E> {
        force <= {
            let+ value = computation;
            <value | force>
        },
    }
}

pub func to_delayed<-T, E>(computation: Lazy<T, ..E>) -> (-> T / {..E}) {
    let- pending = computation.force;
    pending
}
