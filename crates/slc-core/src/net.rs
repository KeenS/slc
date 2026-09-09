//! Lafont-style interaction nets.
//!
//! A net is a multiset of agents connected by wires. Each agent has a
//! principal port (where interaction happens) and auxiliary ports.
//! Reduction occurs when two agents are connected principal-to-principal.

use std::collections::HashMap;
use std::fmt::Write as _;

/// Unique identity of a port in a net.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PortId(pub usize);

/// A port on an agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Port {
    pub agent: usize,
    /// 0 is the principal port; 1.. are auxiliary.
    pub index: usize,
}

/// Agent kinds. Pairs marked `symmetric` interact by swapping their
/// auxiliary connections.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AgentKind {
    /// λ abstraction (function).
    Lam,
    /// μ̃ (value consumer).
    MuTilde,
    /// Tensor constructor (pair).
    Tensor,
    /// Par constructor (dual of tensor).
    Par,
    /// Sum left injection.
    Inl,
    /// Sum right injection.
    Inr,
    /// Fan (sharing node).
    Fan,
    /// Erasure node (discards a value).
    Erase,
    /// Duplicator fan-in.
    Dup,
}

/// An agent in the net.
#[derive(Debug, Clone, PartialEq)]
pub struct Agent {
    pub kind: AgentKind,
    /// Number of auxiliary ports.
    pub arity: usize,
}

/// An interaction net: agents plus a wiring function.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Net {
    pub agents: Vec<Agent>,
    /// wire: port -> port (symmetric; each wire stored once).
    pub wires: Vec<(Port, Port)>,
    /// Free ports (interface of the net).
    pub free: Vec<Port>,
}

impl Port {
    pub fn principal(agent: usize) -> Self {
        Self { agent, index: 0 }
    }

    pub fn aux(agent: usize, index: usize) -> Self {
        Self { agent, index }
    }
}

impl Net {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an agent; returns its index.
    pub fn add_agent(&mut self, kind: AgentKind, arity: usize) -> usize {
        let id = self.agents.len();
        self.agents.push(Agent { kind, arity });
        id
    }

    /// Connect two ports with a wire.
    pub fn connect(&mut self, a: Port, b: Port) {
        self.wires.push((a, b));
    }

    /// Mark a port as free (part of the net interface).
    pub fn mark_free(&mut self, p: Port) {
        self.free.push(p);
    }

    /// Find the port connected to `p`, if any.
    pub fn peer(&self, p: Port) -> Option<Port> {
        for (a, b) in &self.wires {
            if *a == p {
                return Some(*b);
            }
            if *b == p {
                return Some(*a);
            }
        }
        None
    }

    /// Find active pairs: two agents connected principal-to-principal.
    pub fn active_pairs(&self) -> Vec<(usize, usize)> {
        let mut result = Vec::new();
        for (a, b) in &self.wires {
            if a.index == 0 && b.index == 0 {
                result.push((a.agent, b.agent));
            }
        }
        result
    }

    /// Pretty-print the net in dot format.
    pub fn to_dot(&self) -> String {
        let mut out = String::from("digraph net {\n  node [shape=circle];\n");
        for (i, agent) in self.agents.iter().enumerate() {
            let label = match agent.kind {
                AgentKind::Lam => "λ",
                AgentKind::MuTilde => "μ̃",
                AgentKind::Tensor => "⊗",
                AgentKind::Par => "⅋",
                AgentKind::Inl => "inl",
                AgentKind::Inr => "inr",
                AgentKind::Fan => "fan",
                AgentKind::Erase => "ε",
                AgentKind::Dup => "dup",
            };
            let _ = writeln!(out, "  a{i} [label=\"{label}\"];");
        }
        for (i, (a, b)) in self.wires.iter().enumerate() {
            let _ = writeln!(out, "  a{} -> a{} [label=\"w{i}\"];", a.agent, b.agent);
        }
        out.push_str("}\n");
        out
    }
}

/// Result of one net reduction step.
#[derive(Debug, Clone, PartialEq)]
pub enum NetStep {
    /// The net reduced.
    Reduced(Net),
    /// No active pairs remain: normal form.
    Normal,
}

/// Reduce an active pair according to interaction rules.
/// Returns new wires to add (rewiring of auxiliary ports).
pub fn interact(a_kind: AgentKind, b_kind: AgentKind) -> Vec<(usize, usize)> {
    // Each rule returns pairs of auxiliary-port indices to connect.
    match (a_kind, b_kind) {
        // λ meets μ̃: β — connect λ's body to μ̃'s continuation.
        (AgentKind::Lam, AgentKind::MuTilde) => vec![(1, 1)],
        (AgentKind::MuTilde, AgentKind::Lam) => vec![(1, 1)],
        // Tensor meets Par: swap components.
        (AgentKind::Tensor, AgentKind::Par) => vec![(1, 1), (2, 2)],
        (AgentKind::Par, AgentKind::Tensor) => vec![(1, 1), (2, 2)],
        // Inl meets Par: connect first component, erase second.
        (AgentKind::Inl, AgentKind::Par) => vec![(1, 1)],
        (AgentKind::Inr, AgentKind::Par) => vec![(1, 2)],
        (AgentKind::Par, AgentKind::Inl) => vec![(1, 1)],
        (AgentKind::Par, AgentKind::Inr) => vec![(2, 1)],
        // Fan meets fan: symmetric annihilation.
        (AgentKind::Fan, AgentKind::Fan) => vec![(1, 1), (2, 2)],
        // Erase meets anything: no rewiring (value discarded).
        (AgentKind::Erase, _) | (_, AgentKind::Erase) => vec![],
        _ => vec![],
    }
}

/// Perform one reduction step on a net, if possible.
pub fn step(net: &Net) -> NetStep {
    let pairs = net.active_pairs();
    if pairs.is_empty() {
        return NetStep::Normal;
    }
    // Deterministic rule priority: take the first active pair.
    let (a, b) = pairs[0];
    let a_kind = net.agents[a].kind;
    let b_kind = net.agents[b].kind;
    let rewires = interact(a_kind, b_kind);

    let mut new_net = net.clone();
    // Remove the wire between the principals.
    new_net.wires.retain(|(x, y)| {
        !((x.agent == a && y.agent == b && x.index == 0 && y.index == 0)
            || (x.agent == b && y.agent == a && x.index == 0 && y.index == 0))
    });
    // Rewire auxiliary ports according to the interaction rule.
    for (ai, bi) in rewires {
        let pa = Port::aux(a, ai);
        let pb = Port::aux(b, bi);
        // Connect whatever pa and pb were connected to.
        let pa_peer = net.peer(pa);
        let pb_peer = net.peer(pb);
        if let (Some(x), Some(y)) = (pa_peer, pb_peer) {
            new_net.wires.push((x, y));
        } else if let Some(x) = pa_peer {
            new_net.wires.push((x, pb));
        } else if let Some(y) = pb_peer {
            new_net.wires.push((pa, y));
        }
        // Remove the old wires to pa/pb.
        new_net.wires.retain(|(u, v)| {
            !(*u == pa || *v == pa || *u == pb || *v == pb) || (*u == pa && *v == pb)
        });
    }
    NetStep::Reduced(new_net)
}

/// Normalize a net with a fuel budget. Returns None on exhaustion.
pub fn normalize(net: &Net, fuel: usize) -> Option<Net> {
    let mut current = net.clone();
    for _ in 0..fuel {
        match step(&current) {
            NetStep::Reduced(next) => current = next,
            NetStep::Normal => return Some(current),
        }
    }
    None
}

/// Materialize a net into a simple value summary for testing:
/// the kind of the root agent reachable from the free interface.
pub fn root_kind(net: &Net) -> Option<AgentKind> {
    let p = net.free.first()?;
    Some(net.agents[p.agent].kind)
}

/// Simplification pass: remove agents whose ports are all disconnected
/// and unreachable from the interface. This is a conservative garbage
/// collection of dead subnets.
pub fn simplify(net: &Net) -> Net {
    // Compute reachability from free ports via wires.
    let mut reachable: std::collections::HashSet<Port> = net.free.iter().copied().collect();
    let mut changed = true;
    while changed {
        changed = false;
        for (a, b) in &net.wires {
            if reachable.contains(a) && !reachable.contains(b) {
                reachable.insert(*b);
                changed = true;
            }
            if reachable.contains(b) && !reachable.contains(a) {
                reachable.insert(*a);
                changed = true;
            }
        }
    }
    let mut out = Net::new();
    // Map old agent indices to new ones.
    let mut remap: HashMap<usize, usize> = HashMap::new();
    for (i, agent) in net.agents.iter().enumerate() {
        // Keep an agent if any of its ports is reachable.
        let keep = (0..=agent.arity).any(|idx| reachable.contains(&Port { agent: i, index: idx }))
            || net.free.iter().any(|p| p.agent == i);
        if keep {
            let new_id = out.add_agent(agent.kind, agent.arity);
            remap.insert(i, new_id);
        }
    }
    for p in &net.free {
        if let Some(&new_agent) = remap.get(&p.agent) {
            out.mark_free(Port { agent: new_agent, index: p.index });
        }
    }
    for (a, b) in &net.wires {
        if let (Some(&na), Some(&nb)) = (remap.get(&a.agent), remap.get(&b.agent)) {
            out.connect(Port { agent: na, index: a.index }, Port { agent: nb, index: b.index });
        }
    }
    out
}

/// Lamping-style sharing: insert fan nodes to duplicate a shared value.
/// Returns the net with a fan agent connected to the input port and two
/// output ports for the copies.
pub fn share(net: &mut Net, input: Port) -> (Port, Port) {
    let fan = net.add_agent(AgentKind::Fan, 2);
    net.connect(input, Port::principal(fan));
    (Port::aux(fan, 1), Port::aux(fan, 2))
}

/// Check bracket correctness: fans must not overlap incorrectly.
/// In this simplified model, we verify that every fan's auxiliary ports
/// connect to distinct agents (no self-loop through a fan).
pub fn check_brackets(net: &Net) -> bool {
    for agent_id in 0..net.agents.len() {
        if net.agents[agent_id].kind == AgentKind::Fan {
            let p1 = net.peer(Port::aux(agent_id, 1));
            let p2 = net.peer(Port::aux(agent_id, 2));
            if let (Some(a), Some(b)) = (p1, p2)
                && a.agent == b.agent
                && a.index == b.index
            {
                return false;
            }
        }
    }
    true
}

/// Compile a core term into an interaction net.
///
/// Each syntactic construct becomes an agent; cuts become wires between
/// principal ports. Returns the net and the port representing the
/// term's value.
pub fn compile_term(t: &crate::term::Term) -> Net {
    let mut net = Net::new();
    let _root = compile_term_into(t, &mut net);
    net
}

fn compile_term_into(t: &crate::term::Term, net: &mut Net) -> Port {
    use crate::term::Term;
    match t {
        Term::Var(_) => {
            // A variable becomes a free port on an erase agent.
            let a = net.add_agent(AgentKind::Erase, 1);
            let p = Port::aux(a, 1);
            net.mark_free(p);
            p
        }
        Term::Lam(_, body) => {
            // λ agent: principal port, one aux for the parameter,
            // one aux for the body.
            let a = net.add_agent(AgentKind::Lam, 2);
            let param = Port::aux(a, 1);
            net.mark_free(param);
            let body_port = compile_term_into(body, net);
            net.connect(Port::aux(a, 2), body_port);
            Port::principal(a)
        }
        Term::Mu(_, cmd) => {
            let a = net.add_agent(AgentKind::MuTilde, 1);
            let cmd_port = compile_command_into(cmd, net);
            net.connect(Port::aux(a, 1), cmd_port);
            Port::principal(a)
        }
        Term::Pair(t1, t2) => {
            let a = net.add_agent(AgentKind::Tensor, 2);
            let p1 = compile_term_into(t1, net);
            net.connect(Port::aux(a, 1), p1);
            let p2 = compile_term_into(t2, net);
            net.connect(Port::aux(a, 2), p2);
            Port::principal(a)
        }
        Term::Inl(t) => {
            let a = net.add_agent(AgentKind::Inl, 1);
            let p = compile_term_into(t, net);
            net.connect(Port::aux(a, 1), p);
            Port::principal(a)
        }
        Term::Inr(t) => {
            let a = net.add_agent(AgentKind::Inr, 1);
            let p = compile_term_into(t, net);
            net.connect(Port::aux(a, 1), p);
            Port::principal(a)
        }
        Term::Tag(_, payload) => {
            // A labelled injection is an injection; the net keeps the payload
            // wire and forgets the label, which only selects a branch.
            let a = net.add_agent(AgentKind::Inl, 1);
            let p = compile_term_into(payload, net);
            net.connect(Port::aux(a, 1), p);
            Port::principal(a)
        }
        Term::CoAbs(_, body) => {
            let a = net.add_agent(AgentKind::Lam, 1);
            let p = compile_term_into(body, net);
            net.connect(Port::aux(a, 1), p);
            Port::principal(a)
        }
        Term::Co(e) => compile_coterm_into(e, net),
    }
}

fn compile_command_into(c: &crate::command::Command, net: &mut Net) -> Port {
    use crate::command::Command;
    match c {
        Command::Cut(t, e) => {
            let tp = compile_term_into(t, net);
            let ep = compile_coterm_into(e, net);
            net.connect(tp, ep);
            tp
        }
        Command::Command(_, t) => compile_term_into(t, net),
        Command::Activate(k, v) => {
            let kp = compile_term_into(k, net);
            let vp = compile_term_into(v, net);
            net.connect(kp, vp);
            kp
        }
    }
}

fn compile_coterm_into(e: &crate::coterm::CoTerm, net: &mut Net) -> Port {
    use crate::coterm::CoTerm;
    match e {
        CoTerm::Covar(_) => {
            let a = net.add_agent(AgentKind::Erase, 1);
            Port::principal(a)
        }
        CoTerm::CoLam(_, c) => {
            let a = net.add_agent(AgentKind::MuTilde, 1);
            let cp = compile_command_into(c, net);
            net.connect(Port::aux(a, 1), cp);
            Port::principal(a)
        }
        CoTerm::MuTilde(_, c) => {
            let a = net.add_agent(AgentKind::Lam, 1);
            let cp = compile_command_into(c, net);
            net.connect(Port::aux(a, 1), cp);
            Port::principal(a)
        }
        CoTerm::Par(e1, e2) => {
            let a = net.add_agent(AgentKind::Par, 2);
            let p1 = compile_coterm_into(e1, net);
            net.connect(Port::aux(a, 1), p1);
            let p2 = compile_coterm_into(e2, net);
            net.connect(Port::aux(a, 2), p2);
            Port::principal(a)
        }
        CoTerm::MuTildeTensor(binders, body) => {
            let a = net.add_agent(AgentKind::MuTilde, binders.len().max(1));
            let cp = compile_command_into(body, net);
            net.connect(Port::aux(a, 1), cp);
            Port::principal(a)
        }
        CoTerm::CoCase(branches) => {
            // A negative additive consumer is a consumer with one auxiliary
            // wire per branch.
            let a = net.add_agent(AgentKind::MuTilde, branches.len().max(1));
            for (i, branch) in branches.iter().enumerate() {
                let cp = compile_command_into(&branch.body, net);
                net.connect(Port::aux(a, i + 1), cp);
            }
            Port::principal(a)
        }
        CoTerm::Fst | CoTerm::Snd => Port::principal(net.add_agent(AgentKind::Erase, 1)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_net_is_normal() {
        let net = Net::new();
        assert_eq!(step(&net), NetStep::Normal);
    }

    #[test]
    fn active_pair_detection() {
        let mut net = Net::new();
        let a = net.add_agent(AgentKind::Lam, 1);
        let b = net.add_agent(AgentKind::MuTilde, 1);
        net.connect(Port::principal(a), Port::principal(b));
        let pairs = net.active_pairs();
        assert_eq!(pairs, vec![(a, b)]);
    }

    #[test]
    fn lam_meets_mutilde_reduces() {
        let mut net = Net::new();
        let lam = net.add_agent(AgentKind::Lam, 1);
        let mt = net.add_agent(AgentKind::MuTilde, 1);
        net.connect(Port::principal(lam), Port::principal(mt));
        match step(&net) {
            NetStep::Reduced(n) => {
                // The active pair is gone.
                assert!(n.active_pairs().is_empty());
            }
            NetStep::Normal => panic!("expected reduction"),
        }
    }

    #[test]
    fn tensor_par_swap() {
        let rewires = interact(AgentKind::Tensor, AgentKind::Par);
        assert_eq!(rewires, vec![(1, 1), (2, 2)]);
    }

    #[test]
    fn erase_discards() {
        let rewires = interact(AgentKind::Erase, AgentKind::Lam);
        assert!(rewires.is_empty());
    }

    #[test]
    fn normalize_respects_fuel() {
        let net = Net::new();
        assert!(normalize(&net, 10).is_some());
    }

    #[test]
    fn dot_output_contains_agents() {
        let mut net = Net::new();
        let a = net.add_agent(AgentKind::Lam, 1);
        let b = net.add_agent(AgentKind::MuTilde, 1);
        net.connect(Port::principal(a), Port::principal(b));
        let dot = net.to_dot();
        assert!(dot.contains("digraph"));
        assert!(dot.contains("λ"));
        assert!(dot.contains("μ̃"));
    }

    #[test]
    fn compile_identity_lambda() {
        use crate::term::Term;
        let t = Term::Lam("x".into(), Box::new(Term::Var("x".into())));
        let net = compile_term(&t);
        assert!(!net.agents.is_empty());
        assert!(net.agents.iter().any(|a| a.kind == AgentKind::Lam));
    }

    #[test]
    fn compile_pair() {
        use crate::term::Term;
        let t = Term::Pair(Box::new(Term::Var("a".into())), Box::new(Term::Var("b".into())));
        let net = compile_term(&t);
        assert!(net.agents.iter().any(|a| a.kind == AgentKind::Tensor));
    }

    #[test]
    fn compile_cut_creates_wire() {
        use crate::command::Command;
        use crate::coterm::CoTerm;
        use crate::term::Term;
        let t = Term::Lam("x".into(), Box::new(Term::Var("x".into())));
        let e = CoTerm::MuTilde(
            "y".into(),
            Box::new(Command::Cut(Term::Var("y".into()), CoTerm::Covar("k".into()))),
        );
        let net = compile_term(&Term::Mu("m".into(), Box::new(Command::Cut(t, e))));
        assert!(!net.wires.is_empty());
    }

    #[test]
    fn simplify_removes_unreachable() {
        let mut net = Net::new();
        let a = net.add_agent(AgentKind::Lam, 1);
        let b = net.add_agent(AgentKind::MuTilde, 1);
        net.connect(Port::principal(a), Port::principal(b));
        // Mark nothing free: both agents unreachable.
        let simplified = simplify(&net);
        assert!(simplified.agents.is_empty());
    }

    #[test]
    fn simplify_keeps_reachable() {
        let mut net = Net::new();
        let a = net.add_agent(AgentKind::Lam, 1);
        net.mark_free(Port::principal(a));
        let simplified = simplify(&net);
        assert_eq!(simplified.agents.len(), 1);
    }

    #[test]
    fn fan_sharing_creates_two_outputs() {
        let mut net = Net::new();
        let v = net.add_agent(AgentKind::Erase, 1);
        let (o1, o2) = share(&mut net, Port::principal(v));
        assert_ne!(o1, o2);
        assert!(net.agents.iter().any(|a| a.kind == AgentKind::Fan));
    }

    #[test]
    fn brackets_valid_on_simple_net() {
        let mut net = Net::new();
        let a = net.add_agent(AgentKind::Lam, 1);
        let b = net.add_agent(AgentKind::MuTilde, 1);
        net.connect(Port::principal(a), Port::principal(b));
        assert!(check_brackets(&net));
    }
}
