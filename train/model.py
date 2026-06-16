# train/model.py
import torch
from torch import nn

# An LSTM's carried state is a (h, c) tuple, each shape (num_layers, batch, hidden).
LSTMState = tuple[torch.Tensor, torch.Tensor]


class BA283LSTM(nn.Module):
    """Gain-conditioned LSTM emulator with selectable conditioning.

    NAM-faithful default: a 3-layer LSTM(hidden=18) + Dense(1) with a *learned*
    initial hidden/cell state (``h0``/``c0`` trainable ``nn.Parameter``, shape
    ``(num_layers, 1, hidden)``, tiled across the batch when no state is
    supplied), mirroring NAM's ``recurrent.LSTM`` (3 layers, hidden 18, learned
    ``_initial_state``). The earlier shipped graph was a single-layer
    LSTM(hidden=32); the stacked NAM sizing trades per-layer width for depth, and
    the learned initial state removes the zero-init warmup transient.

    The LSTM's extra (forget/cell) gating gives more capacity per unit than the
    GRU it replaced at the same hidden size, attacking the capacity-bound
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

    Redesign knobs (the structural defaults are now NAM-style 3x18; the rest
    default to the linear graph so the export keys stay minimal until set):

    - ``num_layers`` / ``dropout``: stack depth and inter-layer dropout (dropout
      is a no-op at a single layer, matching PyTorch's own behaviour).
    - ``head="mlp"``: insert a ``Linear(H,H) -> tanh`` block (``head_pre``)
      before the dense readout -- a saturating output stage a bare linear readout
      cannot shape.
    - ``film_kind="mlp"``: replace the linear FiLM map with
      ``Linear(1,G) -> tanh -> Linear(G,2H)`` (``G = film_hidden or hidden``),
      giving the conditioning headroom over the ~19 dB gain span.

    At the defaults ``self.film`` is a ``Linear`` and ``self.dense`` a
    ``Linear``, and ``head_pre`` is absent, so the ``state_dict`` carries the
    per-layer LSTM cells, ``h0``/``c0``, ``film.*`` and ``dense.*`` (the
    ``ba283-lstm-v2`` export contract).
    """

    def __init__(
        self,
        hidden: int = 18,
        conditioning: str = "film",
        *,
        num_layers: int = 3,
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
        self.num_layers = num_layers
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
        # Learned initial hidden/cell state (NAM ``_initial_state``): trainable
        # parameters shape (num_layers, 1, hidden), broadcast across the batch
        # when forward() is called without an explicit state. Initialised to
        # zeros so an untrained model matches the old zero-init behaviour.
        self.h0 = nn.Parameter(torch.zeros(num_layers, 1, hidden))
        self.c0 = nn.Parameter(torch.zeros(num_layers, 1, hidden))
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

    def _initial_state(self, batch: int, like: torch.Tensor) -> LSTMState:
        """Tile the learned (h0, c0) across the batch (NAM ``_initial_state``).

        ``expand`` to (num_layers, batch, hidden) then ``contiguous`` because
        cuDNN's LSTM requires a contiguous initial state. ``like`` carries the
        device/dtype of the input so the state matches under AMP / CUDA.
        """
        h0 = self.h0.expand(-1, batch, -1).contiguous().to(like.dtype)
        c0 = self.c0.expand(-1, batch, -1).contiguous().to(like.dtype)
        return h0, c0

    def forward(
        self, x: torch.Tensor, state: LSTMState | None = None
    ) -> tuple[torch.Tensor, LSTMState]:
        # No carried state -> seed with the learned initial (h0, c0) rather than
        # PyTorch's implicit zeros, so the burned-in transient matches training.
        if state is None:
            state = self._initial_state(x.shape[0], x)
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
