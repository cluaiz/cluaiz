# Local CI/CD Testing Tools

Yeh folder tumhare GitHub Actions workflows ko GitHub par push karne se pehle **locally (apne PC par) test karne** ke liye banaya gaya hai. Isse tumhara time bachega aur commit history clean rahegi.

Is folder me 2 scripts hain jo specific kaam karti hain:

## 1. `run-actionlint.ps1`
**Kya karta hai?** 
Yeh sirf 1 second me saari `.github/workflows/*.yml` files ko scan karta hai aur kisi bhi typo, syntax error, ya invalid YAML structure ka pata lagata hai. Isse chalane ke liye Docker ki zaroorat nahi hai.

**Kaise chalayein?**
Terminal (PowerShell) me yeh likho:
```powershell
.\test\workflows\run-actionlint.ps1
```
*Note: Agar output me "0 errors" ya "passed" aaye, toh iska matlab tumhari YAML files perfectly theek hain.*

---

## 2. `run-act.ps1`
**Kya karta hai?**
Yeh tool (`act`) Docker ka use karke tumhare PC par bilkul GitHub jaisa Ubuntu/Windows environment banata hai aur practically tumhari CI/CD pipeline ko compile/run karke test karta hai. Yeh confirm karne ka aakhiri step hai ki code sach me compile hoga.

**Kaise chalayein?**
Sabse pehle **Docker Desktop open karo**, phir terminal me yeh likho:
```powershell
.\test\workflows\run-act.ps1
```
*Note: Yeh pehli baar me kuch GB ki docker images download kar sakta hai, isliye isme thoda time lagega.*
