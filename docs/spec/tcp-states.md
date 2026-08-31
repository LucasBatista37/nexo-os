# TCP no `netd` — estados implementados (v0)

Máquina de estados TCP do serviço de rede residente (`services/netd`), exigida pelo
Plano (§Fase 4: "TCP com suíte de testes e estados documentados"). Lados **ativo** (saída)
e **passivo** (uma escuta/aceitação por vez; sem fila de backlog).

```text
CLOSED ──listen()──▶ LISTEN ──SYN/SYN-ACK──▶ SYN_RCVD ──ACK válido──▶ ESTABLISHED
CLOSED ──connect()/SYN──▶ SYN_SENT ──SYN-ACK válido/ACK──▶ ESTABLISHED
SYN_SENT ──RST──▶ CLOSED (erro 4)          SYN_SENT ──10 s──▶ CLOSED (erro 3)

ESTABLISHED ──close()/FIN──▶ FIN_WAIT_1 ──ACK do FIN──▶ FIN_WAIT_2 ──FIN/ACK──▶ CLOSED
FIN_WAIT_1 ──FIN+ACK juntos──▶ CLOSED
ESTABLISHED ──FIN do par/ACK──▶ CLOSE_WAIT ──close()/FIN──▶ LAST_ACK ──ACK──▶ CLOSED
qualquer estado ──RST──▶ CLOSED (erro 4 na próxima operação)
```

A máquina de estados vive em `nexo-netstack::tcp` (`libraries/net/src/tcp.rs`) e é coberta por
uma suíte de host (handshake, dados nos dois sentidos, retransmissões até RST, fecho ativo e
passivo, RST em qualquer estado, contrapressão de recepção); o `netd` só monta os quadros e
bombeia o temporizador.

Regras da v0 (deliberadas e documentadas):

- **Retransmissão simples**: uma pendência (dados ou SYN/FIN) por vez, reenviada a cada 500 ms
  até 5 vezes; esgotando, a conexão é considerada reiniciada (erro 4). Dados recebidos fora de
  ordem provocam ACK duplicado; sem espaço na janela, não são confirmados (o par retransmite).
  Janela anunciada = espaço livre no buffer de 4 KiB por conexão.
- **TIME_WAIT imediato**: após o fecho ordenado o slot volta a `CLOSED` na hora (portas locais
  41000+i giram por slot; colisão de encarnações é improvável no cenário de teste e será
  tratada com ISNs melhores junto com a retransmissão).
- Sem opções TCP (MSS implícito ≤ 1400 pelos limites do protocolo `nexo.sock`), sem urgência,
  sem *keepalive*, uma pendência de dados por chamada `tcp_send`.
- ISN derivado do relógio monotônico.

Cobertura de teste: suíte de host da máquina de estados e dos segmentos
(`cargo test -p nexo-netstack`) e o cenário `net` (handshake, dados e fecho reais pelo `netd`;
RST no caminho cru do `utest` 14). Escuta/aceitação e janelas deslizantes de verdade são o
próximo passo deste item do plano.
