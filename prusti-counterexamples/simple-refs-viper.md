<h1>simple-refs.vpr</h1>
To analyze how the Silicon counterexample generation works I wrote a very simple viper program:

<details><summary>show program</summary>

```java
field left:Int;
field right:Int;

method simple(x: Ref) returns (n:Int)
    requires acc(x.left)&&acc(x.right)
    requires x.left == 42 && x.right == 52
    ensures acc(x.left)&&acc(x.right)
    ensures n==42 && x.left == 42 && x.right == 52 
    {
        n := x.left
        label ret;
        x.left := 101;
        x.right := 201;
        assert(true);
    } 

```
</details>

<br>
the generated counterexamples look as follows:
<details><summary>show counterexample : variables</summary>

```
Silicon 1.1-SNAPSHOT (2762f4f7+@(detached))

Silicon found 1 error in 3.35s:
  [0] Postcondition of simple might not hold. Assertion x.left == 42 might not hold. (simple-refs.vpr@8.13)
x -> $Ref!val!0
n -> 42
```
</details>
<details><summary>show counterexample : native</summary>

```
Silicon 1.1-SNAPSHOT (2762f4f7+@(detached))

Silicon found 1 error in 3.52s:
  [0] Postcondition of simple might not hold. Assertion x.left == 42 might not hold. (simple-refs.vpr@8.13)
n@3@04 -> 0
$SortWrappers.$RefTo$Snap -> {
    $Snap.unit
}
n@1@04 -> 0
x@0@04 -> $Ref!val!1
$SortWrappers.$SnapTo$Ref -> {
    $Ref!val!1
}
x@2@04 -> $Ref!val!0
$Ref.null -> $Ref!val!1
$SortWrappers.$SnapToInt -> {
    ($Snap.combine $Snap.unit $Snap.unit) -> 52
    else -> 42
}
$SortWrappers.$SnapTo$FVF<$Ref> -> {
    $FVF<$Ref>!val!0
}
$SortWrappers.$SnapToBool -> {
    false
}
n@6@04 -> 42
s@$ -> $Snap.unit
$t@4@04 -> ($Snap.combine $Snap.unit
                          ($Snap.combine ($Snap.combine $Snap.unit $Snap.unit)
                                         ($Snap.combine $Snap.unit $Snap.unit)))
$t@5@04 -> $Snap.unit
$SortWrappers.IntTo$Snap -> {
    52 -> ($Snap.combine $Snap.unit $Snap.unit)
    else -> $Snap.unit
}
$SortWrappers.BoolTo$Snap -> {
    $Snap.unit
}
$SortWrappers.$FVF<$Ref>To$Snap -> {
    $Snap.unit
}
$SortWrappers.$PermTo$Snap -> {
    $Snap.unit
}
$SortWrappers.$SnapTo$Perm -> {
    0.0
}
```
</details>
<br>
<p>
From the variables-counterexample we can not extract anything useful about the values of the counterexample. But also the native model seems to be limited. We would be interested in the values of the fields of x (_1 in viper) and we can find their initial values 42 and 52 in:

    $SortWrappers.$SnapToInt -> {
        ($Snap.combine $Snap.unit $Snap.unit) -> 52
        else -> 42
    }
But later in the program the values of the struct fields are changed to 101 and 201 and they actually cause the verification to fail. But those values appear nowhere in this counterexample. This is possibly a limitation for our implementation because values are not available at all program positions or makes some changes in viper necessary.
