# train/model.py
import torch
from torch import nn


class BA283GRU(nn.Module):
    """Bias-conditioned GRU emulator with FiLM conditioning.

    The plain GRU(2, H) fed the bias as a constant input channel could not
    separate the 5 bias-dependent transfer functions (overfit ESR plateaued at
    ~0.3 even at hidden=96 -- a conditioning, not a capacity, limit). FiLM
    (feature-wise linear modulation) injects the bias far more strongly: the GRU
    runs on the AUDIO alone, and the bias drives a tiny linear map to a per-unit
    affine (gamma, beta) that modulates the GRU's hidden output before the dense
    readout:

        y_t, h = GRU(audio_t, h)              # GRU sees audio only
        gamma, beta = Film(bias_t)            # bias -> 2H affine params
        out_t = Dense((1 + gamma) * y_t + beta)

    The ``1 +`` makes the modulation identity at init (Film starts ~0), so early
    training behaves like an unconditioned GRU and the bias path ramps in.

    The module input is unchanged -- ``x[..., 0]`` is the (scaled) audio and
    ``x[..., 1]`` is the constant bias -- so the dataset/training interface is
    identical. GRU weight keys are preserved (``gru.weight_ih_l0`` etc., now with
    input_size 1) so existing state-dict tooling still works; the extra ``film.*``
    parameters are new.
    """

    def __init__(self, hidden: int = 32) -> None:
        super().__init__()
        self.hidden = hidden
        self.gru = nn.GRU(
            input_size=1,
            hidden_size=hidden,
            num_layers=1,
            batch_first=True,
        )
        self.film = nn.Linear(1, 2 * hidden)
        self.dense = nn.Linear(hidden, 1)

    def forward(
        self, x: torch.Tensor, h: torch.Tensor | None = None
    ) -> tuple[torch.Tensor, torch.Tensor]:
        audio = x[..., 0:1]
        bias = x[..., 1:2]
        y, h_out = self.gru(audio, h)
        gamma, beta = self.film(bias).chunk(2, dim=-1)
        y = (1.0 + gamma) * y + beta
        return self.dense(y), h_out
