# train/ssm.py
"""Pure-PyTorch, recurrent (state-threaded) state-space models for BA283.

Two model families behind one ``BA283SSM`` wrapper that mirrors
:class:`train.model.BA283LSTM`'s constructor and
``forward(x, state) -> (pred, state)`` contract exactly, so the existing TBPTT
training loop, dataset, loss and FiLM conditioning are reused unchanged:

- ``kind="s4d"`` -- diagonal **S4D**. Complex diagonal modes (so the SSM can
  represent resonances, not just real decay) carried in explicit real/imag
  arithmetic (one real state tensor of shape ``(B, H, N, 2)`` per layer). The
  transition is input-independent.
- ``kind="s6"`` -- a reduced **Mamba / S6** block: input proj -> causal
  depthwise conv (state-carrying) -> SiLU gate -> *selective* scan where
  ``dt, B, C`` are projected from the input (input-dependent), ``A`` real
  diagonal. Two state tensors per layer (SSM hidden + conv ring).

Both run as an explicit sequential scan over time with an explicit initial
state in and final state out -- the same streaming contract the LSTM gives, and
what a future real-time/sample-by-sample plugin inference would need. This is
deliberately not the fused-CUDA-kernel formulation: it is portable to any Colab
runtime (CPU or GPU) at the cost of training speed (a Python loop over the
window). Keep windows / HPO epochs modest.

State across the whole stack is a FLAT tuple of tensors (S4D: 1 per layer; S6:
2 per layer, hidden then conv) in layer order. ``train.train`` only does
``tuple(s.detach() for s in state)`` and feeds the same structure back, so a
flat tuple of plain tensors satisfies the TBPTT contract.
"""

from __future__ import annotations

import torch
import torch.nn.functional as F
from torch import nn

SSMState = tuple[torch.Tensor, ...]


class S4DLayer(nn.Module):
    """Diagonal S4D block with complex modes, pre-norm + residual.

    Maps ``(B, L, H) -> (B, L, H)``. The per-channel SSM has ``N`` complex modes;
    the recurrence is ``h_t = dA * h_{t-1} + dB * u_t`` and the output is
    ``2*Re(sum_N C * h_t) + D * u_t``. ``dA``/``dB`` come from a ZOH
    discretisation of a learnable complex diagonal ``A`` with a learnable
    per-channel step ``dt``. State is one real tensor ``(B, H, N, 2)``
    (real/imag stacked) so it threads through TBPTT as a single tensor.
    """

    def __init__(
        self,
        d_model: int,
        d_state: int = 64,
        *,
        dt_min: float = 1e-3,
        dt_max: float = 1e-1,
    ) -> None:
        super().__init__()
        self.d_model = d_model
        self.d_state = d_state
        self.norm = nn.LayerNorm(d_model)
        # A = A_re + i*A_im, A_re < 0 (stable). A_re = -exp(log_A_re).
        self.log_A_re = nn.Parameter(torch.log(0.5 * torch.ones(d_model, d_state)))
        # Imag parts seeded on a spread of frequencies (HiPPO-ish), free to move.
        self.A_im = nn.Parameter(
            torch.pi * torch.arange(d_state).float().repeat(d_model, 1)
        )
        # Complex C (per channel, per mode); B fixed to 1 (folded into C/dt).
        self.C_re = nn.Parameter(torch.randn(d_model, d_state) / d_state**0.5)
        self.C_im = nn.Parameter(torch.randn(d_model, d_state) / d_state**0.5)
        self.D = nn.Parameter(torch.ones(d_model))
        # dt sampled log-uniform in [dt_min, dt_max], stored in log space.
        log_dt = torch.rand(d_model) * (
            torch.log(torch.tensor(dt_max)) - torch.log(torch.tensor(dt_min))
        ) + torch.log(torch.tensor(dt_min))
        self.log_dt = nn.Parameter(log_dt)

    def _discretize(self) -> tuple[torch.Tensor, ...]:
        dt = torch.exp(self.log_dt).unsqueeze(-1)  # (H,1)
        A_re = -torch.exp(self.log_A_re)  # (H,N)
        A_im = self.A_im
        # dA = exp(dt*A) (complex): magnitude*angle.
        mag = torch.exp(dt * A_re)
        ang = dt * A_im
        dA_re = mag * torch.cos(ang)
        dA_im = mag * torch.sin(ang)
        # dB = (dA - 1)/A  (B == 1). Complex division by A = A_re + i A_im.
        num_re = dA_re - 1.0
        num_im = dA_im
        den = A_re * A_re + A_im * A_im + 1e-12
        dB_re = (num_re * A_re + num_im * A_im) / den
        dB_im = (num_im * A_re - num_re * A_im) / den
        return dA_re, dA_im, dB_re, dB_im

    def forward(
        self, u: torch.Tensor, state: torch.Tensor | None = None
    ) -> tuple[torch.Tensor, torch.Tensor]:
        b, length, _ = u.shape
        res = u
        x = self.norm(u)
        dA_re, dA_im, dB_re, dB_im = self._discretize()  # each (H,N)
        if state is None:
            h_re = u.new_zeros(b, self.d_model, self.d_state)
            h_im = u.new_zeros(b, self.d_model, self.d_state)
        else:
            h_re, h_im = state[..., 0], state[..., 1]
        ys = []
        for t in range(length):
            ut = x[:, t, :].unsqueeze(-1)  # (B,H,1)
            new_re = dA_re * h_re - dA_im * h_im + dB_re * ut
            new_im = dA_re * h_im + dA_im * h_re + dB_im * ut
            h_re, h_im = new_re, new_im
            # y = 2*Re(sum_N C * h) + D*u   (factor 2 for the conjugate pair).
            yt = 2.0 * (self.C_re * h_re - self.C_im * h_im).sum(-1)
            yt = yt + self.D * x[:, t, :]
            ys.append(yt)
        y = torch.stack(ys, dim=1)  # (B,L,H)
        new_state = torch.stack([h_re, h_im], dim=-1)  # (B,H,N,2)
        return res + y, new_state


class S6Layer(nn.Module):
    """Reduced Mamba / S6 block (selective scan), pre-norm + residual.

    Maps ``(B, L, H) -> (B, L, H)``. ``dt, B, C`` are projected from the input
    (the "selective" mechanism); ``A`` is a real diagonal. Carries two state
    tensors: the SSM hidden ``(B, d_inner, d_state)`` and the causal-conv ring
    ``(B, d_inner, d_conv-1)``.
    """

    def __init__(
        self,
        d_model: int,
        d_state: int = 16,
        *,
        d_conv: int = 4,
        expand: int = 2,
        dt_rank: int | None = None,
    ) -> None:
        super().__init__()
        self.d_model = d_model
        self.d_state = d_state
        self.d_conv = d_conv
        self.d_inner = expand * d_model
        self.dt_rank = dt_rank or max(1, d_model // 16)
        self.norm = nn.LayerNorm(d_model)
        self.in_proj = nn.Linear(d_model, 2 * self.d_inner)
        self.conv = nn.Conv1d(
            self.d_inner, self.d_inner, kernel_size=d_conv, groups=self.d_inner
        )
        self.x_proj = nn.Linear(self.d_inner, self.dt_rank + 2 * d_state)
        self.dt_proj = nn.Linear(self.dt_rank, self.d_inner)
        self.log_A = nn.Parameter(
            torch.log(torch.arange(1, d_state + 1).float().repeat(self.d_inner, 1))
        )
        self.D = nn.Parameter(torch.ones(self.d_inner))
        self.out_proj = nn.Linear(self.d_inner, d_model)

    def forward(
        self, u: torch.Tensor, state: tuple[torch.Tensor, torch.Tensor] | None = None
    ) -> tuple[torch.Tensor, tuple[torch.Tensor, torch.Tensor]]:
        b, length, _ = u.shape
        res = u
        xz = self.in_proj(self.norm(u))  # (B,L,2*d_inner)
        x, z = xz.chunk(2, dim=-1)
        # --- causal depthwise conv with carried ring state ---
        xt = x.transpose(1, 2)  # (B,d_inner,L)
        if state is None:
            conv_state = xt.new_zeros(b, self.d_inner, self.d_conv - 1)
            h = xt.new_zeros(b, self.d_inner, self.d_state)
        else:
            h, conv_state = state
        padded = torch.cat([conv_state, xt], dim=-1)  # (B,d_inner,d_conv-1+L)
        x_conv = self.conv(padded)  # valid -> (B,d_inner,L)
        new_conv_state = padded[..., -(self.d_conv - 1):]
        x = F.silu(x_conv.transpose(1, 2))  # (B,L,d_inner)
        # --- selective params ---
        x_dbl = self.x_proj(x)
        dt_in, B, C = torch.split(
            x_dbl, [self.dt_rank, self.d_state, self.d_state], dim=-1
        )
        dt = F.softplus(self.dt_proj(dt_in))  # (B,L,d_inner)
        A = -torch.exp(self.log_A)  # (d_inner,d_state)
        ys = []
        for t in range(length):
            dt_t = dt[:, t, :].unsqueeze(-1)  # (B,d_inner,1)
            dA = torch.exp(dt_t * A.unsqueeze(0))  # (B,d_inner,d_state)
            dBx = dt_t * B[:, t, :].unsqueeze(1) * x[:, t, :].unsqueeze(-1)
            h = dA * h + dBx
            yt = (h * C[:, t, :].unsqueeze(1)).sum(-1) + self.D * x[:, t, :]
            ys.append(yt)
        y = torch.stack(ys, dim=1)  # (B,L,d_inner)
        y = y * F.silu(z)
        out = self.out_proj(y)  # (B,L,d_model)
        return res + out, (h, new_conv_state)


class BA283SSM(nn.Module):
    """Gain-conditioned S4D/S6 emulator mirroring :class:`BA283LSTM`.

    Same module interface as the LSTM: ``x`` is ``(B, L, 2)`` (col 0 scaled
    audio, col 1 constant gain); ``forward(x, state) -> (pred, state)`` with
    ``pred`` ``(B, L, 1)``. ``film`` conditioning runs the SSM stack on the audio
    alone and modulates the stack output by ``(1+gamma)*y + beta`` before the
    readout, identical to the LSTM (the ``1 +`` makes FiLM identity at init).
    ``state`` is the flat tuple of every layer's recurrent tensors.
    """

    def __init__(
        self,
        kind: str,
        hidden: int = 48,
        conditioning: str = "film",
        *,
        num_layers: int = 1,
        dropout: float = 0.0,
        head: str = "linear",
        film_kind: str = "linear",
        film_hidden: int | None = None,
        d_state: int = 64,
        d_conv: int = 4,
        expand: int = 2,
    ) -> None:
        super().__init__()
        if kind not in ("s4d", "s6"):
            raise ValueError(f"kind must be 's4d' or 's6', got {kind!r}")
        if conditioning not in ("film", "concat"):
            raise ValueError(f"conditioning must be 'film' or 'concat', got {conditioning!r}")
        if head not in ("linear", "mlp"):
            raise ValueError(f"head must be 'linear' or 'mlp', got {head!r}")
        if film_kind not in ("linear", "mlp"):
            raise ValueError(f"film_kind must be 'linear' or 'mlp', got {film_kind!r}")
        self.kind = kind
        self.hidden = hidden
        self.conditioning = conditioning
        self.head_kind = head
        self.film_kind = film_kind
        ssm_in = 1 if conditioning == "film" else 2
        self.in_proj = nn.Linear(ssm_in, hidden)
        self.dropout = nn.Dropout(dropout) if dropout > 0 else nn.Identity()
        layers = []
        for _ in range(num_layers):
            if kind == "s4d":
                layers.append(S4DLayer(hidden, d_state=d_state))
            else:
                layers.append(
                    S6Layer(hidden, d_state=d_state, d_conv=d_conv, expand=expand)
                )
        self.layers = nn.ModuleList(layers)
        # Each layer contributes a fixed number of state tensors (S4D 1, S6 2).
        self._slots_per_layer = 1 if kind == "s4d" else 2
        if conditioning == "film":
            if film_kind == "linear":
                self.film = nn.Linear(1, 2 * hidden)
            else:
                g = film_hidden or hidden
                self.film = nn.Sequential(
                    nn.Linear(1, g), nn.Tanh(), nn.Linear(g, 2 * hidden)
                )
        if head == "mlp":
            self.head_pre = nn.Linear(hidden, hidden)
        self.dense = nn.Linear(hidden, 1)

    def _readout(self, y: torch.Tensor) -> torch.Tensor:
        if self.head_kind == "mlp":
            y = torch.tanh(self.head_pre(y))
        return self.dense(y)

    def _split_state(self, state: SSMState | None) -> list:
        if state is None:
            return [None] * len(self.layers)
        k = self._slots_per_layer
        per = []
        for i in range(len(self.layers)):
            chunk = state[i * k:(i + 1) * k]
            per.append(chunk[0] if k == 1 else chunk)
        return per

    def forward(
        self, x: torch.Tensor, state: SSMState | None = None
    ) -> tuple[torch.Tensor, SSMState]:
        if self.conditioning == "film":
            u = self.in_proj(x[..., 0:1])
            gain = x[..., 1:2]
        else:
            u = self.in_proj(x)
            gain = None
        in_states = self._split_state(state)
        out_states: list[torch.Tensor] = []
        last = len(self.layers) - 1
        for i, (layer, st) in enumerate(zip(self.layers, in_states, strict=True)):
            u, new_st = layer(u, st)
            # Dropout BETWEEN layers only (never after the last), matching
            # ``nn.LSTM``'s own placement so the SSM stays a faithful A/B against
            # the LSTM baseline it is compared with.
            if i != last:
                u = self.dropout(u)
            if isinstance(new_st, tuple):
                out_states.extend(new_st)
            else:
                out_states.append(new_st)
        if self.conditioning == "film":
            gamma, beta = self.film(gain).chunk(2, dim=-1)
            u = (1.0 + gamma) * u + beta
        return self._readout(u), tuple(out_states)
