# train/model.py
import torch
from torch import nn


class BA283GRU(nn.Module):
    """Bias-conditioned GRU emulator with selectable conditioning.

    Two conditioning modes (for the FiLM-vs-plain A/B):

    - ``"film"`` (default): the GRU runs on the AUDIO alone (input_size 1) and
      the bias drives a tiny linear map to a per-unit affine (gamma, beta) that
      modulates the GRU output before the dense readout::

          y, h = GRU(audio); gamma, beta = Film(bias); out = Dense((1+gamma)*y + beta)

      The ``1 +`` makes modulation identity at init (Film starts ~0).

    - ``"concat"``: the original design -- bias is a second GRU input channel
      (input_size 2), no FiLM. This is the baseline the A/B compares against.

    The module input is identical in both modes: ``x[..., 0]`` is the (scaled)
    audio and ``x[..., 1]`` the constant bias, so the dataset/training interface
    never changes. GRU weight keys (``gru.weight_ih_l0`` etc.) are preserved; the
    ``film.*`` params exist only in film mode.
    """

    def __init__(self, hidden: int = 32, conditioning: str = "film") -> None:
        super().__init__()
        if conditioning not in ("film", "concat"):
            raise ValueError(f"conditioning must be 'film' or 'concat', got {conditioning!r}")
        self.hidden = hidden
        self.conditioning = conditioning
        gru_in = 1 if conditioning == "film" else 2
        self.gru = nn.GRU(
            input_size=gru_in,
            hidden_size=hidden,
            num_layers=1,
            batch_first=True,
        )
        if conditioning == "film":
            self.film = nn.Linear(1, 2 * hidden)
        self.dense = nn.Linear(hidden, 1)

    def forward(
        self, x: torch.Tensor, h: torch.Tensor | None = None
    ) -> tuple[torch.Tensor, torch.Tensor]:
        if self.conditioning == "film":
            audio = x[..., 0:1]
            bias = x[..., 1:2]
            y, h_out = self.gru(audio, h)
            gamma, beta = self.film(bias).chunk(2, dim=-1)
            y = (1.0 + gamma) * y + beta
            return self.dense(y), h_out
        # concat: bias is the second input channel
        y, h_out = self.gru(x, h)
        return self.dense(y), h_out
