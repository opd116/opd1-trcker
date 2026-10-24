<!-- The waiting screen's radar (design v2 `radar`, TrackerApp.dc.html 2070–2076): three rings, a cross, a sweep. -->
<div class="radar" aria-hidden="true">
  <div class="ring" style:inset="0"></div>
  <div class="ring" style:inset="26px"></div>
  <div class="ring" style:inset="52px"></div>
  <div class="v"></div>
  <div class="hz"></div>
  <div class="sweep"></div>
  <div class="c"></div>
</div>

<style>
  .radar {
    position: relative;
    width: 168px;
    height: 168px;
    flex: none;
  }

  .ring {
    position: absolute;
    border-radius: 50%;
    border: 1px solid var(--line-media);
  }

  .v {
    position: absolute;
    left: 50%;
    top: 0;
    bottom: 0;
    width: 1px;
    background: var(--line-soft);
  }

  .hz {
    position: absolute;
    top: 50%;
    left: 0;
    right: 0;
    height: 1px;
    background: var(--line-soft);
  }

  .sweep {
    position: absolute;
    inset: 0;
    border-radius: 50%;
    background: conic-gradient(from 0deg, rgba(157, 140, 255, 0) 0deg, rgba(157, 140, 255, 0) 290deg, rgba(157, 140, 255, 0.32) 360deg);
    animation: opdSweep 3.2s linear infinite;
  }

  .c {
    position: absolute;
    left: 50%;
    top: 50%;
    width: 10px;
    height: 10px;
    margin: -5px 0 0 -5px;
    border-radius: 50%;
    background: var(--accent);
    box-shadow: 0 0 0 4px rgba(157, 140, 255, 0.15);
  }
</style>
