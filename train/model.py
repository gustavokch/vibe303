# train/model.py
import torch
from torch import nn

# An LSTM's carried state is a (h, c) tuple, each shape (num_layers, batch, hidden).
LSTMState = tuple[torch.Tensor, torch.Tensor]


class BA283LSTM(nn.Module):
    """Gain-conditioned LSTM emulator with selectable conditioning.

    A single-layer LSTM(hidden=32) + Dense(1) -- the RTNeural / GuitarML
    cost/quality sweet spot for real-time virtual-analog modelling -- replacing
    the earlier GRU. The LSTM's extra (forget/cell) gating gives more capacity
    per unit than the GRU at the same hidden size, attacking the capacity-bound
    val-ESR plateau the GRU hit.

    Two conditioning modes (for the FiLM-vs-plain A/B):

    - ``"film"`` (default): the LSTM runs on the AUDIO alone (input_size 1) and
      the gain drives a tiny linear map to a per-unit affine (gamma, beta) that
      modulates the LSTM output before the dense readout::

          y, (h,c) = LSTM(audio); gamma, beta = Film(gain); out = Dense((1+gamma)*y + beta)

      The ``1 +`` makes modulation identity at init (Film starts ~0). The core
      LSTM stays input_size 1, so its weight shapes are independent of the
      conditioning scheme.

    - ``"concat"``: gain is a second LSTM input channel (input_size 2), no FiLM.
      The baseline the A/B compares against.

    The module input is identical in both modes: ``x[..., 0]`` is the (scaled)
    audio and ``x[..., 1]`` the constant gain, so the dataset/training interface
    never changes. The carried state is the LSTM ``(h, c)`` tuple; ``film.*``
    params exist only in film mode.

    Redesign knobs (all default to the shipped single-layer / linear graph, so
    the export + Rust round-trip contract is unchanged until a knob is set):

    - ``num_layers`` / ``dropout``: stack depth and inter-layer dropout (dropout
      is a no-op at a single layer, matching PyTorch's own behaviour).
    - ``head="mlp"``: insert a ``Linear(H,H) -> tanh`` block (``head_pre``)
      before the dense readout -- a saturating output stage a bare linear readout
      cannot shape.
    - ``film_kind="mlp"``: replace the linear FiLM map with
      ``Linear(1,G) -> tanh -> Linear(G,2H)`` (``G = film_hidden or hidden``),
      giving the conditioning headroom over the ~19 dB gain span.

    At the defaults ``self.film`` is a ``Linear`` and ``self.dense`` a
    ``Linear``, and ``head_pre`` is absent, so ``state_dict`` keys match the v1
    export (``film.weight``/``film.bias``, ``dense.weight``/``dense.bias``).
    """

    def __init__(
        self,
        hidden: int = 32,
        conditioning: str = "film",
        *,
        num_layers: int = 1,
        dropout: float = 0.0,
        head: str = "linear",
        film_kind: str = "linear",
        film_hidden: int | None = None,
    ) -> None:
        super().__init__()
        if conditioning not in ("film", "concat"):
            raise ValueError(f"conditioning must be 'film' or 'concat', got {conditioning!r}")
        if head not in ("linear", "mlp"):
            raise ValueError(f"head must be 'linear' or 'mlp', got {head!r}")
        if film_kind not in ("linear", "mlp"):
            raise ValueError(f"film_kind must be 'linear' or 'mlp', got {film_kind!r}")
        self.hidden = hidden
        self.conditioning = conditioning
        self.head_kind = head
        self.film_kind = film_kind
        lstm_in = 1 if conditioning == "film" else 2
        self.lstm = nn.LSTM(
            input_size=lstm_in,
            hidden_size=hidden,
            num_layers=num_layers,
            # PyTorch warns (and ignores) dropout at a single layer; gate it off.
            dropout=dropout if num_layers > 1 else 0.0,
            batch_first=True,
        )
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

    def forward(
        self, x: torch.Tensor, state: LSTMState | None = None
    ) -> tuple[torch.Tensor, LSTMState]:
        if self.conditioning == "film":
            audio = x[..., 0:1]
            gain = x[..., 1:2]
            y, state_out = self.lstm(audio, state)
            gamma, beta = self.film(gain).chunk(2, dim=-1)
            y = (1.0 + gamma) * y + beta
            return self._readout(y), state_out
        # concat: gain is the second input channel
        y, state_out = self.lstm(x, state)
        return self._readout(y), state_out
