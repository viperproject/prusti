<h1> simple-enum.rs </h1>

to determine if we can find the values of fields in the counterexample of a simple enum, let's look at this simple rust program with prusti annotations.
<details> <summary> click to show rust program: </summary>

```rust
use prusti_contracts::*;

enum IntOption {
    Some(i32, i32),
    None,
}

#[ensures(result.0 != 42 || result.1 != 55 || result.2 != 66)]
fn foo(x: IntOption, y: IntOption) -> (i32, i32, i32) {
    let k = if let IntOption::Some(z, i) = x { z } else { 45 };
    if let IntOption::Some(s, t) = y {
        (k + 1, s + 1, t + 1)
    } else {
        (0, 0, 0)
    }
}

fn main() {}
```

</details>
<br>

<h2> silicon counterxample </h2>
the generated native silicon counterexample is very large! It is all displayed in the next collapsed section but further down you can find all the information we were able to extract.
<details><summary>full native silicon counterexample</summary>

```
Silicon 1.1-SNAPSHOT (2762f4f7+@(detached))

Silicon found 1 error in 14.37s:
  [0] Assert might fail. Assertion !((unfolding acc(tuple3$i32$i32$i32(_0), write) in (unfolding acc(i32(_0.tuple_0), write) in _0.tuple_0.val_int)) == 42) || (!((unfolding acc(tuple3$i32$i32$i32(_0), write) in (unfolding acc(i32(_0.tuple_1), write) in _0.tuple_1.val_int)) == 55) || !((unfolding acc(tuple3$i32$i32$i32(_0), write) in (unfolding acc(i32(_0.tuple_2), write) in _0.tuple_2.val_int)) == 66)) might not hold. (simple-enum.rs.vpr@575.3)
$SortWrappers.$RefTo$Snap -> {
    $Ref!val!11 -> ($Snap.combine $Snap.unit $Snap.unit)
    else -> $Snap.unit
}
$t@60@03 -> $Snap.unit
closure$0_13$3$15912831434393096257%trigger -> {
    false
}
__t7@9@03 -> false
$t@57@03 -> $Snap.unit
ret@59@03 -> 0
_14@33@03 -> 0
m_simple_enum$$IntOption$_beg_$_end_%trigger -> {
    true
}
_8@27@03 -> 0
ret@62@03 -> 0
_9@28@03 -> 0
DeadBorrowToken$%trigger -> {
    false
}
m_simple_enum$$IntOption$_beg_$_end_$$discriminant$$__$TY$__m_simple_enum$$IntOption$_beg_$_end_$$int$ -> {
    0
}
__t2@4@03 -> false
$SortWrappers.$SnapTo$Ref -> {
    ($Snap.combine $Snap.unit $Snap.unit) -> $Ref!val!11
    else -> $Ref!val!2
}
_4@23@03 -> 0
$t@52@03 -> $Snap.unit
tuple3$i32$i32$i32%trigger -> {
    true
}
__t13@15@03 -> 0
__t0@2@03 -> false
__t14@17@03 -> false
m_simple_enum$$IntOption$_beg_$_end_$$discriminant$$__$TY$__m_simple_enum$$IntOption$_beg_$_end_$$int$%stateless -> {
    true
}
__t3@5@03 -> false
_1@20@03 -> $Ref!val!0
$t@41@03 -> $Snap.unit
__t16@19@03 -> false
$Ref.null -> $Ref!val!1
_2@21@03 -> $Ref!val!3
val_int@77@03 -> 55
$t@84@03 -> false
_17@36@03 -> 0
$t@79@03 -> $Snap.unit
$SortWrappers.$SnapToInt -> {
    ($Snap.combine $Snap.unit ($Snap.combine $Snap.unit $Snap.unit)) -> 41
    ($Snap.combine ($Snap.combine $Snap.unit $Snap.unit) $Snap.unit) -> 66
    ($Snap.combine $Snap.unit
               ($Snap.combine $Snap.unit ($Snap.combine $Snap.unit $Snap.unit))) -> 42
    ($Snap.combine ($Snap.combine $Snap.unit ($Snap.combine $Snap.unit $Snap.unit))
               $Snap.unit) -> 65
    ($Snap.combine ($Snap.combine ($Snap.combine $Snap.unit $Snap.unit) $Snap.unit)
               $Snap.unit) -> 54
    ($Snap.combine $Snap.unit $Snap.unit) -> 55
    else -> 0
}
tuple2$i32$bool%trigger -> {
    false
}
$SortWrappers.$SnapTo$FVF<$Ref> -> {
    $FVF<$Ref>!val!0
}
__t9@11@03 -> false
$t@83@03 -> $Ref!val!9
_18@37@03 -> $Ref!val!7
__t15@18@03 -> false
m_simple_enum$$IntOption$_beg_$_end_Some%trigger -> {
    true
}
tuple0$%trigger -> {
    false
}
_3@22@03 -> 0
_0@1@03 -> $Ref!val!7
__t11@13@03 -> false
$SortWrappers.$SnapToBool -> {
    false
}
_aux_havoc_i32@16@03 -> $Ref!val!7
ref$closure$0_13$3$15912831434393096257%trigger -> {
    false
}
$t@68@03 -> false
__t12@14@03 -> 0
_9@61@03 -> 65
$t@43@03 -> $Snap.unit
val_int@69@03 -> 42
__t6@8@03 -> false
_4@44@03 -> 0
m_simple_enum$$IntOption$_beg_$_end_$$discriminant$$__$TY$__m_simple_enum$$IntOption$_beg_$_end_$$int$%limited -> {
    0
}
$t@75@03 -> $Ref!val!7
joined_unfolding@5@00 -> {
    0
}
_15@34@03 -> $Ref!val!7
$t@39@03 -> $Snap.unit
ret@80@03 -> $Ref!val!8
$t@40@03 -> $Snap.unit
_5@24@03 -> 0
s@$ -> $Snap.unit
$t@82@03 -> 0
$t@49@03 -> $Snap.unit
$t@38@03 -> ($Snap.combine ($Snap.combine $Snap.unit
                                      ($Snap.combine $Snap.unit
                                                     ($Snap.combine $Snap.unit
                                                                    ($Snap.combine $Snap.unit
                                                                                   ($Snap.combine $Snap.unit
                                      ($Snap.combine ($Snap.combine $Snap.unit
                                                                    ($Snap.combine $Snap.unit
                                                                                   $Snap.unit))
                                                     ($Snap.combine $Snap.unit
                                                                    $Snap.unit))))))) ($Snap.combine $Snap.unit ($Snap.combine $Snap.unit
                                      ($Snap.combine $Snap.unit
                                                     ($Snap.combine $Snap.unit
                                                                    ($Snap.combine $Snap.unit
                                                                                   ($Snap.combine ($Snap.combine ($Snap.combine ($Snap.combine $Snap.unit
                                                                                   $Snap.unit)
                                                                    $Snap.unit)
                                                     $Snap.unit)
                                      ($Snap.combine $Snap.unit
                                      ($Snap.combine ($Snap.combine $Snap.unit
                                                                    ($Snap.combine $Snap.unit
                                                                                   $Snap.unit))
                                                     $Snap.unit)))))))))
_5@47@03 -> 41
_16@35@03 -> $Ref!val!7
ret@45@03 -> 0
_7@26@03 -> 0
_0@0@03 -> $Ref!val!7
_13@32@03 -> $Ref!val!7
ret@48@03 -> 0
read$%stateless -> true
isize%trigger -> {
    false
}
ref$tuple3$i32$i32$i32%trigger -> {
    false
}
_6@25@03 -> 0
ret@72@03 -> $Ref!val!6
ret@56@03 -> 0
__t5@7@03 -> false
__t8@10@03 -> false
$SortWrappers.IntTo$Snap -> {
    55 -> ($Snap.combine $Snap.unit $Snap.unit)
    42 -> ($Snap.combine $Snap.unit
               ($Snap.combine $Snap.unit ($Snap.combine $Snap.unit $Snap.unit)))
    41 -> ($Snap.combine $Snap.unit ($Snap.combine $Snap.unit $Snap.unit))
    54 -> ($Snap.combine ($Snap.combine ($Snap.combine $Snap.unit $Snap.unit) $Snap.unit)
               $Snap.unit)
    65 -> ($Snap.combine ($Snap.combine $Snap.unit ($Snap.combine $Snap.unit $Snap.unit))
               $Snap.unit)
    66 -> ($Snap.combine ($Snap.combine $Snap.unit $Snap.unit) $Snap.unit)
    else -> $Snap.unit
}
$t@71@03 -> $Snap.unit
ret@78@03 -> 0
$t@66@03 -> 0
_6@50@03 -> 0
bool%trigger -> {
    false
}
__t10@12@03 -> false
__t4@6@03 -> false
$t@54@03 -> $Snap.unit
$t@76@03 -> false
__t1@3@03 -> false
_10@29@03 -> $Ref!val!7
$t@74@03 -> 0
$SortWrappers.BoolTo$Snap -> {
    $Snap.unit
}
$t@87@03 -> ($Snap.combine $Snap.unit ($Snap.combine $Snap.unit
                                      ($Snap.combine $Snap.unit
                                                     ($Snap.combine $Snap.unit
                                                                    ($Snap.combine $Snap.unit
                                                                                   $Snap.unit)))))
_11@30@03 -> 0
$t@67@03 -> $Ref!val!5
$SortWrappers.$FVF<$Ref>To$Snap -> {
    $Snap.unit
}
$t@81@03 -> $Ref!val!2
ret@51@03 -> 0
ret@42@03 -> 0
_8@58@03 -> 54
$SortWrappers.$PermTo$Snap -> {
    $Snap.unit
}
read$%limited -> {
    (/ 1.0 2.0)
}
ret@86@03 -> $Ref!val!10
_7@55@03 -> 0
ret@70@03 -> 0
$t@46@03 -> $Snap.unit
_12@31@03 -> $Ref!val!7
$t@65@03 -> $Ref!val!2
$t@63@03 -> $Snap.unit
ret@53@03 -> 0
$SortWrappers.$SnapTo$Perm -> {
    0.0
}
read$ -> {
    (/ 1.0 2.0)
}
val_int@85@03 -> 66
i32%trigger -> {
    true
}
ret@64@03 -> $Ref!val!4
$t@73@03 -> $Ref!val!11

```
</details>
<details><summary>silicon counterexample variables</summary>

```
Silicon found 1 error in 14.82s:
  [0] Assert might fail. Assertion !((unfolding acc(tuple3$i32$i32$i32(_0), write) in (unfolding acc(i32(_0.tuple_0), write) in _0.tuple_0.val_int)) == 42) || (!((unfolding acc(tuple3$i32$i32$i32(_0), write) in (unfolding acc(i32(_0.tuple_1), write) in _0.tuple_1.val_int)) == 55) || !((unfolding acc(tuple3$i32$i32$i32(_0), write) in (unfolding acc(i32(_0.tuple_2), write) in _0.tuple_2.val_int)) == 66)) might not hold. (simple-enum.rs.vpr@575.3)
_aux_havoc_i32 -> $Ref!val!7
_1 -> $Ref!val!0
_2 -> $Ref!val!3
_4 -> 0
__t12 -> 0
_5 -> 41
_6 -> 0
_3 -> 41
_7 -> 0
__t13 -> 0
_8 -> 54
_9 -> 65
_11 -> 41
_12 -> $Ref!val!4
_10 -> $Ref!val!2
_14 -> 54
_15 -> $Ref!val!6
_13 -> $Ref!val!11
_17 -> 65
_18 -> $Ref!val!8
_16 -> $Ref!val!2
_0 -> $Ref!val!10

```

</details>
<br>
<h2>extracted information</h2>
there is obviously a lot of information in the native counterexample and it is very hard to understand, at least for an inexperienced user. But since we know what to expect from our postcondition that fails we can still extract some things without completely understanding it.
<br>
<p>
1. <b> enum discriminant</b> : in this example, both enums passed to the function have to be IntOption::Some(x,y) for some values x, y to violate the postcondition. Their type is defined by their "discriminant" which can be found in this field:

```
 m_simple_enum$$IntOption$_beg_$_end_$$discriminant$$__$TY$__m_simple_enum$$IntOption$_beg_$_end_$$int$%limited -> {
    0
}
```
<br>   
        where this field always returns 0 (for both enums). If we modify our program such that the postcondition is violated if the first enum is IntOption::None, the counterexample gives us this value:  
<br>
<br>
<details>

```
m_simple_enum$$IntOption$_beg_$_end_$$discriminant$$__$TY$__m_simple_enum$$IntOption$_beg_$_end_$$int$%limited -> {
    ($Snap.combine ($Snap.combine ($Snap.combine $Snap.unit $Snap.unit)
                                $Snap.unit)
                 ($Snap.combine $Snap.unit ($Snap.combine $Snap.unit ($Snap.combine $Snap.unit
                          ($Snap.combine $Snap.unit
                                         ($Snap.combine ($Snap.combine ($Snap.combine ($Snap.combine ($Snap.combine $Snap.unit
                                                        ($Snap.combine $Snap.unit
                                                                       $Snap.unit))
                                         $Snap.unit)
                          $Snap.unit)
                                                                       $Snap.unit)
                                                        ($Snap.combine $Snap.unit
                                                                       ($Snap.combine ($Snap.combine ($Snap.combine ($Snap.combine $Snap.unit
                                                                       $Snap.unit)
                                                        $Snap.unit)
                                         $Snap.unit)
                          $Snap.unit)))))))) $Ref!val!1 -> 0
    else -> 1
```
</details>
<br>
    While I do not yet know how to interpret all of these Snap.unit / Snap.combine, we can see that now the values of this field were 0 and 1 where as it was always 0 before, because now one of the inputs is None and the other is Some. So from this field in the counterexample we could probably determine the types of our enums.

</p>
<p>
2. <b>values of fields of enums and tuples</b>: in this example we have our 2 enums x and y with 2 fields each and a result that is a tuple of 3 values. We know that the counterexample has to look something like this: 
    
- x <- IntOption::Some(41, ?),
- y <- IntOption::Some(54, 64),
- result <- (42, 55, 65)

to violate the postcondition. We can find all of these value in another one of these Snap-fields of the counterexample:
<details>
    
```
$SortWrappers.$SnapToInt -> {
($Snap.combine $Snap.unit ($Snap.combine $Snap.unit $Snap.unit)) -> 41
($Snap.combine ($Snap.combine $Snap.unit $Snap.unit) $Snap.unit) -> 66
($Snap.combine $Snap.unit
               ($Snap.combine $Snap.unit ($Snap.combine $Snap.unit $Snap.unit))) -> 42
($Snap.combine ($Snap.combine $Snap.unit ($Snap.combine $Snap.unit $Snap.unit))
               $Snap.unit) -> 65
($Snap.combine ($Snap.combine ($Snap.combine $Snap.unit $Snap.unit) $Snap.unit)
               $Snap.unit) -> 54
($Snap.combine $Snap.unit $Snap.unit) -> 55
else -> 0
    }
```
</details>
    How to exactly extract and map them is still to be resolved but at least I am sure now that they are present so it should be possible, which would already be a large improvement over what can be extracted from silicons "variable" counterexamples. 
    
</p>  
<p> 
<b>note:</b> in this viper program the variables _1 and _2 map to x and y, and _0 maps to the result.    
