# قائمة الفحص: OPD1 TRCKER v0.3.1

جرّب على جهازك ومعك اللعبة. كل بند سطر واحد. أي شي يطلع غلط، أرسل لي صورة أو نص الـ log (Ctrl+Shift+L).

## التثبيت

1. نزّل `OPD1-TRCKER-0.3.1-setup.exe` من صفحة Releases في GitHub، وشغّله فوق النسخة القديمة. إذا طلع تحذير من ويندوز: More info ثم Run anyway.
2. افتح البرنامج: My Stats والذاكرة القديمة (⟲ وسجل Details) كلها باقية.

## الجديد

3. My Stats → Session: مرّر الماوس على أي مربع W/L: يطلع الماب والـ RR وكم صار له. اضغط عليه: تفتح تفاصيل الماتش، وBack يرجعك لنفس المكان.
4. My Stats → رسم الـ RR: اضغط Tab لين توصل للرسم، وحرّك بالأسهم يمين ويسار: يطلع تلميح كل نقطة.
5. Ctrl+Shift+L والصقه لي: أول سطر لازم يكون فيه رقم النسخة 0.3.1 ونسخة ويندوز والسيرفر.

## اللي كان يشتغل قبل (تأكد إنه باقي تمام)

6. Lobby وAgent Select وIn Game مثل قبل، بدون scroll، والكروت تتعبّى بسرعة.
7. اضغط على أي كرت: Details، وShow more، وMatch Details.
8. My Stats كاملة، وتتحدث بعد كل ماتش.

9. صورة لأي شي شكله غلط، أو أي شي حسيته بطيء.

## بعد تغيّر الآكت (14 أكتوبر)

10. هذي النسخة ما فيها شغل الآكت الجديد (أجّلناه). لو شفت الرانك `—` بعد 14 أكتوبر لين تلعب، هذا متوقع. أرسل لي Ctrl+Shift+L ذاك اليوم عشان نبني الحل على بيانات حقيقية.

---

## للمتحكم (Claude): الـ dumps اللي نفحصها لو أرسلها المستخدم

هذا القسم مو للمستخدم.

- **سطر الـ header** في أول الـ log (R182): النسخة، Windows build، الـ stage، الـ shard، مصدر نسخة العميل، والآكت ومصدره.
- **pregame `CharacterSelectionState`**: القفل يقرأ `locked` (R99).
- **`AccountLevel` حق المستخدم** لما `HideAccountLevel` يكون true: قيمته مو صفر (R100).
- **شكل loadouts في pregame**: `Loadouts[].Subject` (spec §7.3.6).
- **`player_loadout.json`**: `Guns[].ID/SkinID/ChromaID` (R90, v3 حسب R148).
- **هل match-details يرجّع أسماء للاعبين المخفيين** (R74, R94):

  ```sql
  SELECT game_name, tag_line FROM match_players WHERE match_id = '<that match>' AND puuid = '<puuid>'
  ```

- **`competitiveupdates` بالصفحات** (R173): ما في 429 متكرر على `/mmr/`، و`seeded request holds` (R185) يطلع بس بعد 429.
- **أول رد content-service** بعد 14 أكتوبر (R162): الآكت الجديد `IsActive`. وهل `mmr` فيه `SeasonalInfoBySeasonID` للآكت الجديد قبل أول ماتش (يحدد R188 المؤجل).
- **رقم الـ RAM** مقابل هدف الخفّة.
