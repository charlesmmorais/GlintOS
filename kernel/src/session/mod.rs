//! Sessao: a unidade de execucao do GlintOS.
//!
//! Nao e' processo. E' a menor coisa escalonavel, orcavel e preemptivel
//! do sistema. Ver docs/arquitetura secao 3.1.
//!
//! ESQUELETO: os tipos estao definidos e o ciclo de vida esta declarado.
//! O motor de inferencia que os preenche chega no M1.

use crate::accel::CostModel;

pub type SessionId = u32;

/// Orcamento e' capacidade, nao sugestao. Sessao sem orcamento
/// nao e' escalonavel (invariante I4).
#[derive(Clone, Copy, Debug)]
pub struct Budget {
    pub tokens_max: u32,
    pub tokens_used: u32,
    pub wall_ns_max: u64,
    pub wall_ns_used: u64,
}

impl Budget {
    pub const fn new(tokens_max: u32, wall_ns_max: u64) -> Self {
        Self { tokens_max, tokens_used: 0, wall_ns_max, wall_ns_used: 0 }
    }

    pub fn exhausted(&self) -> bool {
        self.tokens_used >= self.tokens_max || self.wall_ns_used >= self.wall_ns_max
    }

    pub fn tokens_left(&self) -> u32 {
        self.tokens_max.saturating_sub(self.tokens_used)
    }
}

/// Motivo de recusa. Sempre legivel: `glint run` imprime isso ao usuario,
/// e nunca ha recusa silenciosa (principio 3 e secao 7).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refused {
    /// Nao cabe no prazo pedido dado o modelo de custo do acelerador.
    DeadlineUnreachable,
    /// Nao ha reserva de vazao disponivel.
    NoThroughputReserve,
    /// Nao ha memoria para o KV cache minimo desta sessao.
    InsufficientKvMemory,
    /// Pesos solicitados nao estao residentes e nao ha como carrega-los.
    WeightsUnavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionState {
    Admitted,
    Running,
    /// Preemptada em fronteira de token (I3). KV preservado.
    Preempted,
    /// KV despejado sob pressao. Retomavel ao custo de recomputacao.
    Evicted,
    Done,
    Refused(Refused),
}

/// Referencia a pesos: objeto compartilhado, imutavel, residente uma unica vez.
#[derive(Clone, Copy, Debug)]
pub struct WeightsRef {
    pub id: u32,
}

pub struct Session {
    pub id: SessionId,
    pub weights: WeightsRef,
    pub kv: crate::kv::KvHandle,
    pub budget: Budget,
    pub deadline_ns: Option<u64>,
    pub state: SessionState,
    /// Vazao reservada na admissao. O escalonador honra isto.
    pub reserved_tokens_per_sec: u32,
    /// I6: progresso e' observavel. Zerado a cada token emitido.
    pub last_heartbeat_ns: u64,
}

impl Session {
    pub fn new(
        id: SessionId,
        weights: WeightsRef,
        kv: crate::kv::KvHandle,
        budget: Budget,
        reserved_tokens_per_sec: u32,
    ) -> Self {
        Self {
            id,
            weights,
            kv,
            budget,
            deadline_ns: None,
            state: SessionState::Admitted,
            reserved_tokens_per_sec,
            last_heartbeat_ns: 0,
        }
    }

    /// Custo estimado desta sessao ate o fim do orcamento.
    pub fn estimated_remaining_ns(&self, cost: &CostModel) -> u64 {
        cost.ns_per_token.saturating_mul(self.budget.tokens_left() as u64)
    }

    pub fn is_schedulable(&self) -> bool {
        matches!(self.state, SessionState::Admitted | SessionState::Running | SessionState::Preempted)
            && !self.budget.exhausted()
    }
}
