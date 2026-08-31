//! Política de rede por perfil (Fase 4: "firewall por aplicativo e perfil" e "permissões de
//! rede por pacote"). O `netd` associa a cada sessão de cliente um [`Profile`] e consulta
//! [`Profile::allows`] antes de abrir uma conexão ou enviar um datagrama — o modelo do Nexo é
//! **negar por padrão**: uma sessão sem capacidades não fala com ninguém. Sem alocação;
//! testável no host.

/// Uma regra de destino permitido: sub-rede IPv4 (`base`/`prefix`) e faixa de portas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rule {
    /// Base da sub-rede (IPv4).
    pub base: [u8; 4],
    /// Bits de prefixo significativos (0..=32; 0 = qualquer endereço).
    pub prefix: u8,
    /// Menor porta permitida (inclusive).
    pub port_lo: u16,
    /// Maior porta permitida (inclusive).
    pub port_hi: u16,
    /// Protocolos: bit 0 = TCP, bit 1 = UDP.
    pub protos: u8,
}

/// Protocolo TCP na máscara de `Rule::protos`.
pub const PROTO_TCP: u8 = 1;
/// Protocolo UDP na máscara de `Rule::protos`.
pub const PROTO_UDP: u8 = 2;
/// Máximo de regras por perfil.
pub const MAX_RULES: usize = 8;

impl Rule {
    fn matches(&self, ip: [u8; 4], port: u16, proto: u8) -> bool {
        if self.protos & proto == 0 || port < self.port_lo || port > self.port_hi {
            return false;
        }
        if self.prefix == 0 {
            return true;
        }
        let bits = self.prefix.min(32) as u32;
        let mask: u32 = if bits == 32 {
            u32::MAX
        } else {
            !((1u32 << (32 - bits)) - 1)
        };
        let a = u32::from_be_bytes(ip) & mask;
        let b = u32::from_be_bytes(self.base) & mask;
        a == b
    }
}

/// Perfil de rede: lista de regras de permissão (negar por padrão) + permissões de serviço.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Profile {
    rules: [Rule; MAX_RULES],
    count: usize,
    /// Pode resolver nomes (DNS).
    pub allow_dns: bool,
    /// Pode abrir sockets em escuta (conexões de entrada).
    pub allow_listen: bool,
}

/// Motivo de uma negação (para diagnóstico/log).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Deny {
    /// Nenhuma regra cobre este destino.
    NoRule,
    /// O protocolo não é permitido por nenhuma regra do destino.
    Protocol,
    /// DNS não permitido.
    Dns,
    /// Escuta não permitida.
    Listen,
}

impl Profile {
    /// Perfil vazio: nega tudo (o padrão do Nexo).
    pub const fn deny_all() -> Self {
        Profile {
            rules: [Rule {
                base: [0; 4],
                prefix: 0,
                port_lo: 0,
                port_hi: 0,
                protos: 0,
            }; MAX_RULES],
            count: 0,
            allow_dns: false,
            allow_listen: false,
        }
    }

    /// Perfil aberto (para o cliente de sistema/testes): TCP+UDP para qualquer destino, DNS e escuta.
    pub fn unrestricted() -> Self {
        let mut p = Profile::deny_all();
        p.rules[0] = Rule {
            base: [0; 4],
            prefix: 0,
            port_lo: 0,
            port_hi: u16::MAX,
            protos: PROTO_TCP | PROTO_UDP,
        };
        p.count = 1;
        p.allow_dns = true;
        p.allow_listen = true;
        p
    }

    /// Acrescenta uma regra; `false` se o perfil já está cheio.
    pub fn add_rule(&mut self, rule: Rule) -> bool {
        if self.count >= MAX_RULES {
            return false;
        }
        self.rules[self.count] = rule;
        self.count += 1;
        true
    }

    /// Uma conexão/datagrama para `(ip, port, proto)` é permitido?
    pub fn allows(&self, ip: [u8; 4], port: u16, proto: u8) -> Result<(), Deny> {
        for r in &self.rules[..self.count] {
            if r.matches(ip, port, proto) {
                return Ok(());
            }
        }
        // distingue "protocolo" de "sem regra" para um diagnóstico melhor
        let any_addr = self.rules[..self.count].iter().any(|r| {
            r.prefix == 0 || {
                let bits = r.prefix.min(32) as u32;
                let mask = if bits == 32 {
                    u32::MAX
                } else {
                    !((1u32 << (32 - bits)) - 1)
                };
                (u32::from_be_bytes(ip) & mask) == (u32::from_be_bytes(r.base) & mask)
            }
        });
        Err(if any_addr {
            Deny::Protocol
        } else {
            Deny::NoRule
        })
    }

    /// Resolução de nomes é permitida?
    pub fn allows_dns(&self) -> Result<(), Deny> {
        if self.allow_dns {
            Ok(())
        } else {
            Err(Deny::Dns)
        }
    }

    /// Escuta (conexões de entrada) é permitida?
    pub fn allows_listen(&self) -> Result<(), Deny> {
        if self.allow_listen {
            Ok(())
        } else {
            Err(Deny::Listen)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deny_all_blocks_everything() {
        let p = Profile::deny_all();
        assert_eq!(p.allows([10, 0, 2, 2], 80, PROTO_TCP), Err(Deny::NoRule));
        assert_eq!(p.allows_dns(), Err(Deny::Dns));
        assert_eq!(p.allows_listen(), Err(Deny::Listen));
    }

    #[test]
    fn unrestricted_allows_all() {
        let p = Profile::unrestricted();
        assert!(p.allows([1, 2, 3, 4], 443, PROTO_TCP).is_ok());
        assert!(p.allows([10, 0, 2, 2], 53, PROTO_UDP).is_ok());
        assert!(p.allows_dns().is_ok());
        assert!(p.allows_listen().is_ok());
    }

    #[test]
    fn subnet_and_port_and_proto() {
        let mut p = Profile::deny_all();
        // so HTTPS para 10.0.2.0/24, TCP
        p.add_rule(Rule {
            base: [10, 0, 2, 0],
            prefix: 24,
            port_lo: 443,
            port_hi: 443,
            protos: PROTO_TCP,
        });
        assert!(p.allows([10, 0, 2, 2], 443, PROTO_TCP).is_ok());
        // porta errada -> Protocol (o endereco casa)
        assert_eq!(p.allows([10, 0, 2, 2], 80, PROTO_TCP), Err(Deny::Protocol));
        // UDP nao permitido -> Protocol
        assert_eq!(p.allows([10, 0, 2, 2], 443, PROTO_UDP), Err(Deny::Protocol));
        // fora da sub-rede -> NoRule
        assert_eq!(p.allows([10, 0, 3, 5], 443, PROTO_TCP), Err(Deny::NoRule));
        // prefixo /24: 10.0.2.255 dentro, 10.0.3.0 fora
        assert!(p.allows([10, 0, 2, 255], 443, PROTO_TCP).is_ok());
        assert_eq!(p.allows([10, 0, 3, 0], 443, PROTO_TCP), Err(Deny::NoRule));
    }

    #[test]
    fn prefix_edges() {
        let mut p = Profile::deny_all();
        p.add_rule(Rule {
            base: [93, 184, 216, 34],
            prefix: 32,
            port_lo: 1,
            port_hi: 65535,
            protos: PROTO_TCP | PROTO_UDP,
        });
        assert!(p.allows([93, 184, 216, 34], 80, PROTO_TCP).is_ok());
        assert_eq!(
            p.allows([93, 184, 216, 35], 80, PROTO_TCP),
            Err(Deny::NoRule)
        );
        // /0 casa qualquer coisa
        let mut q = Profile::deny_all();
        q.add_rule(Rule {
            base: [0; 4],
            prefix: 0,
            port_lo: 8000,
            port_hi: 9000,
            protos: PROTO_TCP,
        });
        assert!(q.allows([1, 1, 1, 1], 8080, PROTO_TCP).is_ok());
        assert_eq!(q.allows([1, 1, 1, 1], 80, PROTO_TCP), Err(Deny::Protocol));
    }

    #[test]
    fn rules_are_capped() {
        let mut p = Profile::deny_all();
        for i in 0..MAX_RULES {
            assert!(p.add_rule(Rule {
                base: [10, 0, i as u8, 0],
                prefix: 24,
                port_lo: 80,
                port_hi: 80,
                protos: PROTO_TCP,
            }));
        }
        assert!(!p.add_rule(Rule {
            base: [0; 4],
            prefix: 0,
            port_lo: 0,
            port_hi: 0,
            protos: PROTO_TCP,
        }));
    }
}
