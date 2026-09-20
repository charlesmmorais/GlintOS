//! KV cache como objeto de kernel.
//!
//! A peca mais importante do projeto (secao 3.3). Paginado em blocos de
//! tamanho fixo, com tabela de blocos por sessao -- PagedAttention DENTRO
//! do kernel, nao por cima dele.
//!
//! Tres consequencias que so sao naturais quando e' o kernel que gerencia:
//!   1. despejo com modelo de custo (recomputar vs. reter);
//!   2. compartilhamento de prefixo por copy-on-write entre sessoes;
//!   3. camadas (HBM / DRAM / NVMe).
//!
//! ESQUELETO: estrutura e politica declaradas; o preenchimento chega no M1.

/// Tamanho de bloco. DECISAO EM ABERTO (secao 12): amarra ao mesmo tempo a
/// granularidade de despejo e a de COW de prefixo. Escolher errado custa caro.
pub const KV_BLOCK_TOKENS: usize = 16;

pub type BlockId = u32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tier {
    Dram,
    // Hbm,   -- quando houver acelerador com memoria propria
    // Nvme,  -- M5
}

/// Bloco fisico do cache. `refcount > 1` significa prefixo compartilhado
/// entre sessoes: copy-on-write na divergencia.
#[derive(Clone, Copy)]
pub struct Block {
    pub phys_addr: u64,
    pub tier: Tier,
    pub refcount: u16,
    /// Custo de recomputar este bloco, em nanossegundos. Entrada do
    /// modelo de despejo: descartar o que e' barato de refazer.
    pub recompute_cost_ns: u64,
    pub last_touch_ns: u64,
}

/// Referencia da sessao ao seu cache. A tabela de blocos e' indirecao:
/// e' ela que torna COW de prefixo e despejo parcial possiveis.
#[derive(Clone, Copy, Debug)]
pub struct KvHandle {
    pub id: u32,
    pub block_count: u32,
}

impl KvHandle {
    pub const fn empty(id: u32) -> Self {
        Self { id, block_count: 0 }
    }

    pub fn bytes(&self, bytes_per_block: u64) -> u64 {
        self.block_count as u64 * bytes_per_block
    }
}

/// Politica de despejo.
///
/// Camada 1 do esquema de "IA como politica" (secao 6): esta e' a politica
/// deterministica padrao, que SEMPRE existe e sempre funciona. O hook de
/// politica aprendida propoe; o validador verifica; se falhar ou estourar
/// prazo, cai aqui sem drama.
pub struct EvictionPolicy;

impl EvictionPolicy {
    /// Pontua um bloco para despejo. Maior pontuacao = candidato melhor.
    ///
    /// O kernel tem os dois numeros que importam -- custo de recomputacao e
    /// tempo desde o ultimo toque. Uma biblioteca em espaco de usuario
    /// tem no maximo o segundo.
    pub fn score(block: &Block, now_ns: u64) -> u64 {
        let age = now_ns.saturating_sub(block.last_touch_ns);
        // Barato de refazer e ha muito nao tocado: despeja primeiro.
        age.saturating_div(block.recompute_cost_ns.max(1))
    }
}
