# TCP no `netd` — estados implementados (v0)

Máquina de estados do cliente TCP do serviço de rede residente (`services/netd`), exigida pelo
Plano (§Fase 4: "TCP com suíte de testes e estados documentados"). Só o lado **ativo**
(conexões de saída) existe nesta versão; escuta/aceitação vem depois.

```text
CLOSED ──connect()/SYN──▶ SYN_SENT ──SYN-ACK válido/ACK──▶ ESTABLISHED
SYN_SENT ──RST──▶ CLOSED (erro 4)          SYN_SENT ──10 s──▶ CLOSED (erro 3)

ESTABLISHED ──close()/FIN──▶ FIN_WAIT_1 ──ACK do FIN──▶ FIN_WAIT_2 ──FIN/ACK──▶ CLOSED
FIN_WAIT_1 ──FIN+ACK juntos──▶ CLOSED
ESTABLISHED ──FIN do par/ACK──▶ CLOSE_WAIT ──close()/FIN──▶ LAST_ACK ──ACK──▶ CLOSED
qualquer estado ──RST──▶ CLOSED (erro 4 na próxima operação)
```

Regras da v0 (deliberadas e documentadas):

- **Sem retransmissão própria**: segmentos enviados uma vez; dados recebidos fora de ordem ou
  sem espaço na janela **não são confirmados**, forçando o par a retransmitir. Janela anunciada
  = espaço livre no buffer de 4 KiB por conexão.
- **TIME_WAIT imediato**: após o fecho ordenado o slot volta a `CLOSED` na hora (portas locais
  41000+i giram por slot; colisão de encarnações é improvável no cenário de teste e será
  tratada com ISNs melhores junto com a retransmissão).
- Sem opções TCP (MSS implícito ≤ 1400 pelos limites do protocolo `nexo.sock`), sem urgência,
  sem *keepalive*, uma pendência de dados por chamada `tcp_send`.
- ISN derivado do relógio monotônico.

Cobertura de teste: cenário `net` (handshake, dados nos dois sentidos, fecho com FIN a partir
do cliente e RST no caminho cru do `utest` 14) e testes de host dos segmentos
(`cargo test -p nexo-netstack`). Retransmissão, escuta e uma suíte dedicada de estados são o
próximo passo deste item do plano.
