from typing import Tuple
from nagini_contracts.contracts import *

def max(x: Tuple[int, int]) -> int:
  Ensures(Result() > x[0] or Result() > x[1])
  if x[0] > x[1]:
    return x[0]
  else:
    return x[1] 
