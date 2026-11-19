# Z#++ and Z#

Zsharpend is Z#++ continued, and Z#++ continued Sam-astro's Z#. Both still run.

- **Z#++ files** (`.zsp`, `.zsx`, `.zsg`) load exactly as before. Zsharpend's new keywords are reserved only in `.zxx` files.
- **Z# programs** (`.zs`, or `.zsp`) run unchanged: `func Main()`, typed declarations, `global`, `Printl`, `ZS.Math.*`, `ZS.Graphics.Init`.

```zsp run
func Main()
{
    int loop = 3
    string case = "Z# style"
    while loop > 0
    {
        Printl(case + " " + loop)
        loop -= 1
    }
}
```

A Whetstone project can use a `.zsp` as its entry (`entry = "main.zsp"`), and a `.zxx` program can `import "old.zsp"`.

## Moving a Z#++ program to .zxx

Rename it to `.zxx` and run `whetstone check`. What can change:

- names that are now keywords (`fn`, `enum`, `trait`, `loop`, `elif`, `switch`, `case`, `mut`, `from`, `None`, `True`, `False`) need renaming;
- a string on its own line now **prints** (HolyC), and a function's bare name now **calls** it;
- assigning to an undeclared name still declares it, with a warning.
