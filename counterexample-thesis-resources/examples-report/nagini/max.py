from nagini_contracts.contracts import *

def max(x: int, y: int) -> int:
  Ensures(Result() > x or Result() > y)
  if x > y:
    return x
  else:
    return y 
