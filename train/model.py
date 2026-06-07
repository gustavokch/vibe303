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
    """

    def __init__(self, hidden: int = 32, conditioning: str = "film") -> None:
        super().__init__()
        if conditioning not in ("film", "concat"):
            raise ValueError(f"conditioning must be 'film' or 'concat', got {conditioning!r}")
        self.hidden = hidden
        self.conditioning = conditioning
        lstm_in = 1 if conditioning == "film" else 2
        self.lstm = nn.LSTM(
            input_size=lstm_in,
            hidden_size=hidden,
            num_layers=1,
            batch_first=True,
        )
        if conditioning == "film":
            self.film = nn.Linear(1, 2 * hidden)
        self.dense = nn.Linear(hidden, 1)

    def forward(
        self, x: torch.Tensor, state: LSTMState | None = None
    ) -> tuple[torch.Tensor, LSTMState]:
        if self.conditioning == "film":
            audio = x[..., 0:1]
            gain = x[..., 1:2]
            y, state_out = self.lstm(audio, state)
            gamma, beta = self.film(gain).chunk(2, dim=-1)
            y = (1.0 + gamma) * y + beta
            return self.dense(y), state_out
        # concat: gain is the second input channel
        y, state_out = self.lstm(x, state)
        return self.dense(y), state_out
