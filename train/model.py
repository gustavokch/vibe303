# train/model.py
import torch
from torch import nn


class BA283GRU(nn.Module):
    def __init__(self, hidden: int = 32) -> None:
        super().__init__()
        self.hidden = hidden
        self.gru = nn.GRU(
            input_size=2,
            hidden_size=hidden,
            num_layers=1,
            batch_first=True,
        )
        self.dense = nn.Linear(hidden, 1)

    def forward(
        self, x: torch.Tensor, h: torch.Tensor | None = None
    ) -> tuple[torch.Tensor, torch.Tensor]:
        y, h_out = self.gru(x, h)
        return self.dense(y), h_out
