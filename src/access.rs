//! Acessibilidade: a árvore que os leitores de tela (Orca, NVDA, Narrador, VoiceOver) leem,
//! pelo AccessKit. Ela sai do mesmo desenho do quadro: cada widget que importa se registra no
//! [`DrawCx`](crate::DrawCx), e só quando há um leitor ligado.
pub use accesskit;
use crate::Rect;
use accesskit::{Affine, Node, NodeId, Role, TreeId, TreeInfo, TreeUpdate};
use std::collections::HashSet;

/// O nó da janela; os widgets usam a chave do caminho deles na árvore.
pub(crate) const RAIZ: NodeId = NodeId(u64::MAX);

/// O que os widgets de um quadro registraram.
#[derive(Default)]
pub struct Access {
    nos: Vec<(u64, Node, Option<usize>)>,
    /// Os nós abertos (botão, rolagem) e se juntam o texto de dentro no rótulo deles.
    abertos: Vec<(Option<usize>, bool)>,
    chaves: HashSet<u64>,
    /// O rótulo de um [`Element::access_label`](crate::Element::access_label) esperando o próximo nó.
    pub(crate) rotulo: Option<String>,
}

impl Access {
    pub(crate) fn push(&mut self, key: u64, r: Rect, mut node: Node, abrir: bool) {
        let pai = self.abertos.last().and_then(|a| a.0);
        // Dois registros com a mesma chave quebrariam a árvore: o segundo some, os filhos sobem.
        if !self.chaves.insert(key) {
            if abrir {
                self.abertos.push((pai, false));
            }
            return;
        }
        if let Some(l) = self.rotulo.take() {
            node.set_label(l);
        }
        node.set_bounds(accesskit::Rect { x0: r.x as f64, y0: r.y as f64, x1: (r.x + r.width) as f64, y1: (r.y + r.height) as f64 });
        let junta = node.label().is_none() && matches!(node.role(), Role::Button | Role::Link | Role::Tab | Role::MenuItem);
        self.nos.push((key, node, pai));
        if abrir {
            self.abertos.push((Some(self.nos.len() - 1), junta));
        }
    }

    pub(crate) fn end(&mut self) {
        self.abertos.pop();
    }

    /// Texto solto vira rótulo; dentro de um botão, vira o nome do botão.
    pub(crate) fn text(&mut self, key: u64, r: Rect, s: &str) {
        if s.trim().is_empty() {
            return;
        }
        if let Some(&(Some(i), true)) = self.abertos.last() {
            let n = &mut self.nos[i].1;
            let l = n.label().map_or_else(|| s.to_owned(), |l| format!("{l} {s}"));
            n.set_label(l);
            return;
        }
        let mut n = Node::new(Role::Label);
        n.set_value(s);
        self.push(key, r, n, false);
    }

    /// A árvore inteira para o AccessKit. `scale` leva as áreas (lógicas) para os pixels da
    /// janela; `focus` é a chave do widget com foco ([`States::focus`](crate::States)).
    pub fn tree(self, title: &str, scale: f64, focus: Option<u64>) -> TreeUpdate {
        let mut filhos: Vec<Vec<NodeId>> = vec![Vec::new(); self.nos.len()];
        let mut raiz = Vec::new();
        for (key, _, pai) in &self.nos {
            match pai {
                Some(p) => filhos[*p].push(NodeId(*key)),
                None => raiz.push(NodeId(*key)),
            }
        }
        let mut janela = Node::new(Role::Window);
        janela.set_label(title);
        janela.set_children(raiz);
        janela.set_transform(Affine::scale(scale));
        let foco = focus.filter(|k| self.chaves.contains(k)).map_or(RAIZ, NodeId);
        let mut nodes = vec![(RAIZ, janela)];
        nodes.extend(self.nos.into_iter().zip(filhos).map(|((k, mut n, _), f)| {
            if !f.is_empty() {
                n.set_children(f);
            }
            (NodeId(k), n)
        }));
        let info = TreeInfo { root: RAIZ, toolkit_name: Some("pelicano".into()), toolkit_version: Some(env!("CARGO_PKG_VERSION").into()) };
        TreeUpdate { nodes, tree: Some(info), tree_id: TreeId::ROOT, focus: foco }
    }
}
