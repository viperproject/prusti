method Max(x:int, y:int) returns (n:int)
    ensures (n > x || n > y)
    {
        if (x>y){
            n := x;
        } else {
            n := y;
        }
    }




