// Shared Overhead Sky Dome, 100° Horizon Vantage, Live Weather/Tide Overlays,
// and 48-Hour Sparkline Canvas Renderers for Tec11.

let hostGetActiveView = () => null;
let hostRefreshView = () => {};

export function registerSkyVantageHost(getActiveViewFn, refreshViewFn) {
  if (typeof getActiveViewFn === "function") {
    hostGetActiveView = getActiveViewFn;
  }
  if (typeof refreshViewFn === "function") {
    hostRefreshView = refreshViewFn;
  }
}

export function rgbFloatToCss(rgb, fallbackR = 255, fallbackG = 230, fallbackB = 180) {
  if (!rgb || rgb.length < 3) {
    return `rgb(${fallbackR}, ${fallbackG}, ${fallbackB})`;
  }
  const r = Math.round(Math.max(0, Math.min(255, rgb[0] * 255)));
  const g = Math.round(Math.max(0, Math.min(255, rgb[1] * 255)));
  const b = Math.round(Math.max(0, Math.min(255, rgb[2] * 255)));
  return `rgb(${r}, ${g}, ${b})`;
}

function eclipticToEquatorial(lamRad, betaRad, epsRad) {
  const cosB = Math.cos(betaRad);
  const xEcl = cosB * Math.cos(lamRad);
  const yEcl = cosB * Math.sin(lamRad);
  const zEcl = Math.sin(betaRad);
  const cosE = Math.cos(epsRad);
  const sinE = Math.sin(epsRad);
  return [xEcl, yEcl * cosE - zEcl * sinE, yEcl * sinE + zEcl * cosE];
}

function raDecDegToEquatorial(raDeg, decDeg) {
  const ra = (raDeg * Math.PI) / 180.0;
  const dec = (decDeg * Math.PI) / 180.0;
  const cosD = Math.cos(dec);
  return [cosD * Math.cos(ra), cosD * Math.sin(ra), Math.sin(dec)];
}

function equatorialToHorizontal(eqVec, siderealRad, latRad) {
  const cosT = Math.cos(siderealRad);
  const sinT = Math.sin(siderealRad);
  const xm = eqVec[0] * cosT + eqVec[1] * sinT;
  const ym = -eqVec[0] * sinT + eqVec[1] * cosT;
  const zm = eqVec[2];
  const sinPhi = Math.sin(latRad);
  const cosPhi = Math.cos(latRad);
  const east = ym;
  const north = -sinPhi * xm + cosPhi * zm;
  const up = cosPhi * xm + sinPhi * zm;
  return [east, north, up];
}

function projectHorizontalToDome(horiz, cx, cy, radius) {
  const east = horiz[0];
  const north = horiz[1];
  const up = Math.max(-1.0, Math.min(1.0, horiz[2]));
  const alt = Math.asin(up);
  const rFrac = (Math.PI * 0.5 - alt) / (Math.PI * 0.5);
  const hLen = Math.hypot(east, north);
  const ux = hLen > 1e-6 ? east / hLen : 0.0;
  const uy = hLen > 1e-6 ? north / hLen : 0.0;
  return {
    x: cx + radius * rFrac * ux,
    y: cy - radius * rFrac * uy,
    up,
    alt,
    rFrac,
  };
}

function sunEquatorialAtDays(sky, totalDays, epsRad) {
  const daysPerYear = Math.max(1.0, sky.homePeriodDays || sky.daysPerYear || 365.0);
  const lamSun = (sky.homeInitAngleRad || 0.0) + (2.0 * Math.PI * totalDays) / daysPerYear;
  return eclipticToEquatorial(lamSun, 0.0, epsRad);
}

function moonEquatorialAtDays(moon, totalDays, epsRad) {
  const period = Math.max(1.0, moon.periodDays || 27.32);
  const incRad = ((moon.inclinationDeg || 5.1) * Math.PI) / 180.0;
  const lam = (moon.initAngleRad || 0.0) + (2.0 * Math.PI * totalDays) / period;
  const node = (moon.nodeAscRad || 0.0) - (2.0 * Math.PI * totalDays) / (period * 248.0);
  const beta = Math.asin(Math.sin(incRad) * Math.sin(lam - node));
  return eclipticToEquatorial(lam, beta, epsRad);
}

function planetEquatorialAtDays(planet, sky, totalDays, epsRad) {
  const homeP = Math.max(1.0, sky.homePeriodDays || sky.daysPerYear || 365.0);
  const homeR = Math.max(0.02, sky.homeOrbitAu || 1.0);
  const thHome = (sky.homeInitAngleRad || 0.0) + (2.0 * Math.PI * totalDays) / homeP;
  const hx = homeR * Math.cos(thHome);
  const hy = homeR * Math.sin(thHome);

  const pPeriod = Math.max(1.0, planet.periodDays || 365.0);
  const pR = Math.max(0.02, planet.orbitAu || 1.5);
  const thP = (planet.initAngleRad || 0.0) + (2.0 * Math.PI * totalDays) / pPeriod;
  const incRad = ((planet.inclinationDeg || 1.8) * Math.PI) / 180.0;
  const px = pR * Math.cos(thP);
  const py = pR * Math.sin(thP) * Math.cos(incRad);
  const pz = pR * Math.sin(thP) * Math.sin(incRad);

  const dx = px - hx;
  const dy = py - hy;
  const dz = pz;
  const dist = Math.hypot(dx, dy, dz) || 1.0;
  const lam = Math.atan2(dy, dx);
  const beta = Math.asin(Math.max(-1.0, Math.min(1.0, dz / dist)));
  return eclipticToEquatorial(lam, beta, epsRad);
}

function drawDiurnalArc(ctx, sampleEqFn, siderealMidday, latRad, cx, cy, radius, strokeStyle, lineWidth, dashed) {
  const steps = 48;
  ctx.save();
  ctx.strokeStyle = strokeStyle;
  ctx.lineWidth = lineWidth;
  if (dashed) {
    ctx.setLineDash([4, 3]);
  }
  ctx.beginPath();
  let penDown = false;
  for (let i = 0; i <= steps; i++) {
    const tau = i / steps;
    const sid = siderealMidday + 2.0 * Math.PI * (tau - 0.5);
    const eq = sampleEqFn(tau);
    const horiz = equatorialToHorizontal(eq, sid, latRad);
    if (horiz[2] >= -0.015) {
      const pt = projectHorizontalToDome(horiz, cx, cy, radius);
      if (!penDown) {
        ctx.moveTo(pt.x, pt.y);
        penDown = true;
      } else {
        ctx.lineTo(pt.x, pt.y);
      }
    } else {
      penDown = false;
    }
  }
  ctx.stroke();
  ctx.restore();
}

export function wrapDeltaAzDeg(azDeg, centerAzDeg) {
  let d = ((azDeg - centerAzDeg) % 360.0 + 540.0) % 360.0 - 180.0;
  if (d === -180.0) d = 180.0;
  return d;
}

export function vantageCurrentNeighbor(view) {
  if (!view || !view.tileVantage || !Array.isArray(view.tileVantage.neighbors)) {
    return null;
  }
  const nbs = view.tileVantage.neighbors;
  if (nbs.length === 0) return null;
  const idx = ((view.tileVantageNeighborIdx | 0) % nbs.length + nbs.length) % nbs.length;
  return {
    index: idx,
    count: nbs.length,
    azDeg: nbs[idx].azDeg || 0.0,
    compass: nbs[idx].compass || "N",
    label: nbs[idx].label || "",
    neighbor: nbs[idx],
  };
}

export function vantageSummaryStrings(view) {
  const cur = vantageCurrentNeighbor(view);
  if (!cur) {
    return {
      bearingSummary: "Horizon (100°)",
      neighborSummary: "",
    };
  }
  const azRound = Math.round(((cur.azDeg % 360.0) + 360.0) % 360.0);
  if (view.tileVantage && view.tileVantage.obsIsCoarse) {
    const distKm = Math.max(20, Math.round(view.tileVantage.ring1DistKm || 300));
    return {
      bearingSummary: `${cur.compass} ${azRound}° (${cur.index + 1}/${cur.count})`,
      neighborSummary: `Facing ${cur.compass} (${azRound}°) — Local tile horizon only (coarse neighbors ~${distKm} km away, beyond horizon)`,
    };
  }
  return {
    bearingSummary: `${cur.compass} ${azRound}° (${cur.index + 1}/${cur.count})`,
    neighborSummary: `Facing ${cur.compass} (${azRound}°): ${cur.label}`,
  };
}

function resolveSkyTimeState(view, sky) {
  const daysPerYear = Math.max(1, sky.daysPerYear || 365);
  const live = view.tileLiveEnv || null;
  if (live && live.simLocked) {
    const year = Math.max(0, live.simYear | 0);
    const dayFloat = Math.max(0.0, live.simDay || 0.0);
    const day = Math.floor(dayFloat) % daysPerYear;
    const tau = Math.max(0.0, Math.min(0.9999, typeof live.simHourFrac === "number" ? live.simHourFrac : dayFloat % 1.0));
    return { daysPerYear, year, day, tau, live };
  }
  const year = Math.max(0, view.tileSkyYear || 0);
  const day = Math.max(0, Math.min(daysPerYear - 1, view.tileSkyDay || 0));
  const tau = typeof view.tileSkyHourFrac === "number" ? view.tileSkyHourFrac : 0.42;
  return { daysPerYear, year, day, tau, live };
}

export function paintTileSkyCanvas(view) {
  const canvas = document.getElementById("tile-sky-canvas");
  if (!canvas || !view || !view.tileSky) return;
  const sky = view.tileSky;

  const rect = canvas.getBoundingClientRect();
  const cssW = Math.max(170, Math.round(rect.width || 210));
  const cssH = Math.max(190, Math.round(rect.height || 224));
  const dpr = window.devicePixelRatio || 1;
  const targetW = Math.round(cssW * dpr);
  const targetH = Math.round(cssH * dpr);
  if (canvas.width !== targetW || canvas.height !== targetH) {
    canvas.width = targetW;
    canvas.height = targetH;
  }

  const ctx = canvas.getContext("2d");
  if (!ctx) return;
  ctx.save();
  ctx.scale(dpr, dpr);

  ctx.fillStyle = "#040814";
  ctx.fillRect(0, 0, cssW, cssH);

  const cx = cssW * 0.5;
  const cy = cssH * 0.51;
  const radius = Math.min(cssW, cssH) * 0.425;

  const latRad = ((sky.latDeg || 0.0) * Math.PI) / 180.0;
  const epsRad = ((sky.tiltDeg || 23.5) * Math.PI) / 180.0;
  const { daysPerYear, year, day, tau, live } = resolveSkyTimeState(view, sky);

  const cloudLow = live ? Math.max(0.0, Math.min(1.0, live.cloudLow || 0.0)) : 0.0;
  const cloudHigh = live ? Math.max(0.0, Math.min(1.0, live.cloudHigh || 0.0)) : 0.0;
  const stormDark = live ? Math.max(0.0, Math.min(1.0, live.stormDarkening || 0.0)) : 0.0;
  const rainMmHr = live ? Math.max(0.0, live.rainMmHr || 0.0) : 0.0;
  const snowMmHr = live ? Math.max(0.0, live.snowMmHr || 0.0) : 0.0;
  const windAzRad = live ? ((live.windAzDeg || 250.0) * Math.PI) / 180.0 : 4.2;
  const cloudTrans = (1.0 - 0.80 * cloudLow) * (1.0 - 0.34 * cloudHigh) * (1.0 - 0.45 * stormDark);

  const baseDays = year * daysPerYear + day;
  const totalDays = baseDays + tau;

  // Determine solar noon RA on this day so tau=0.5 is local solar noon
  const sunNoonEq = sunEquatorialAtDays(sky, baseDays + 0.5, epsRad);
  const raNoon = Math.atan2(sunNoonEq[1], sunNoonEq[0]);
  const siderealNow = raNoon + 2.0 * Math.PI * (tau - 0.5);

  const sunEqNow = sunEquatorialAtDays(sky, totalDays, epsRad);
  const sunHorizNow = equatorialToHorizontal(sunEqNow, siderealNow, latRad);
  const sunAltDeg = (Math.asin(Math.max(-1.0, Math.min(1.0, sunHorizNow[2]))) * 180.0) / Math.PI;
  const sunDecDeg = (Math.asin(Math.max(-1.0, Math.min(1.0, sunEqNow[2]))) * 180.0) / Math.PI;

  // Update live time & solar declination readout
  const timeEl = document.getElementById("tile-sky-time-label");
  if (timeEl) {
    const dayHours = Math.max(2.0, sky.dayHours || 24.0);
    const totalHours = tau * dayHours;
    const hh = String(Math.floor(totalHours)).padStart(2, "0");
    const mm = String(Math.floor((totalHours % 1.0) * 60.0)).padStart(2, "0");
    const signDec = sunDecDeg >= 0 ? "+" : "";
    timeEl.textContent = `${hh}:${mm} | Sun Dec ${signDec}${sunDecDeg.toFixed(1)}°`;
  }

  // Dynamic sky dome color based on sun altitude, PlanetaryOptics, and live overcast/storm darkening
  const hasAtm = sky.hasAtmosphere !== false;
  const dayFactor = Math.max(0.0, Math.min(1.0, (sunAltDeg + 6.0) / 24.0));
  const twiFactor = Math.max(0.0, Math.min(1.0, 1.0 - Math.abs(sunAltDeg + 3.0) / 11.0));
  const zRgb = sky.skyZenithRgb || [0.16, 0.38, 0.82];
  const hRgb = sky.skyHorizonRgb || [0.48, 0.72, 0.96];
  const sRgb = sky.sunsetRgb || [0.95, 0.45, 0.22];
  const tRgb = sky.twilightRgb || [0.18, 0.22, 0.45];

  const overcastMix = hasAtm ? Math.min(0.88, cloudLow * 0.72 + stormDark * 0.42) : 0.0;
  const stormDim = 1.0 - stormDark * 0.48;
  const overcastDayLum = (0.14 + 0.56 * dayFactor) * stormDim;

  const mixChannel = (nightV, dayV, twiV, greyBias) => {
    const clearV =
      nightV * (1.0 - dayFactor) + dayV * dayFactor * 0.85 + twiV * twiFactor * 0.45;
    const greyV = overcastDayLum * greyBias;
    return Math.round(
      Math.max(0, Math.min(255, (clearV * (1.0 - overcastMix) + greyV * overcastMix) * 255.0))
    );
  };

  const zenColor = hasAtm
    ? `rgb(${mixChannel(0.02, zRgb[0], tRgb[0], 0.74)}, ${mixChannel(0.04, zRgb[1], tRgb[1], 0.79)}, ${mixChannel(0.11, zRgb[2], tRgb[2], 0.88)})`
    : "rgb(4, 7, 15)";
  const horColor = hasAtm
    ? `rgb(${mixChannel(0.05, hRgb[0], sRgb[0], 0.80)}, ${mixChannel(0.08, hRgb[1], sRgb[1], 0.84)}, ${mixChannel(0.17, hRgb[2], sRgb[2], 0.91)})`
    : "rgb(9, 13, 24)";

  // Clip inside the local sky dome
  ctx.save();
  ctx.beginPath();
  ctx.arc(cx, cy, radius, 0, Math.PI * 2);
  ctx.clip();

  const grad = ctx.createRadialGradient(cx, cy, radius * 0.05, cx, cy, radius);
  grad.addColorStop(0.0, zenColor);
  grad.addColorStop(1.0, horColor);
  ctx.fillStyle = grad;
  ctx.fillRect(cx - radius, cy - radius, radius * 2.0, radius * 2.0);

  // Altitude rings (30° and 60°) and N-S / E-W crosshairs
  ctx.strokeStyle = "rgba(148, 163, 184, 0.22)";
  ctx.lineWidth = 0.8;
  for (const frac of [1.0 / 3.0, 2.0 / 3.0]) {
    ctx.beginPath();
    ctx.arc(cx, cy, radius * frac, 0, Math.PI * 2);
    ctx.stroke();
  }
  ctx.beginPath();
  ctx.moveTo(cx - radius, cy);
  ctx.lineTo(cx + radius, cy);
  ctx.moveTo(cx, cy - radius);
  ctx.lineTo(cx, cy + radius);
  ctx.stroke();

  // 1. Background stars & Constellation asterisms + diurnal star paths
  const starAlpha = (hasAtm ? Math.max(0.24, 1.0 - dayFactor * 0.65) : 0.94) * Math.max(0.08, cloudTrans);
  if (sky.constellations && starAlpha > 0.05) {
    for (const c of sky.constellations) {
      const cCenterEq = raDecDegToEquatorial(c.raDeg, c.decDeg);
      drawDiurnalArc(
        ctx,
        () => cCenterEq,
        raNoon,
        latRad,
        cx,
        cy,
        radius,
        `rgba(56, 189, 248, ${(0.16 * starAlpha).toFixed(3)})`,
        0.75,
        true
      );

      const projStars = (c.stars || []).map((st) => {
        const eq = raDecDegToEquatorial(st[0], st[1]);
        const horiz = equatorialToHorizontal(eq, siderealNow, latRad);
        return { ...projectHorizontalToDome(horiz, cx, cy, radius), mag: st[2] || 1.8 };
      });

      ctx.strokeStyle = `rgba(56, 189, 248, ${(0.62 * starAlpha).toFixed(3)})`;
      ctx.lineWidth = 1.1;
      for (const edge of c.edges || []) {
        const a = projStars[edge[0]];
        const b = projStars[edge[1]];
        if (a && b && (a.up >= -0.02 || b.up >= -0.02)) {
          ctx.beginPath();
          ctx.moveTo(a.x, a.y);
          ctx.lineTo(b.x, b.y);
          ctx.stroke();
        }
      }

      for (const ps of projStars) {
        if (ps.up < -0.01) continue;
        const rStar = Math.max(1.2, 2.7 - 0.45 * ps.mag);
        ctx.fillStyle = `rgba(241, 245, 249, ${(0.95 * starAlpha).toFixed(3)})`;
        ctx.beginPath();
        ctx.arc(ps.x, ps.y, rStar, 0, Math.PI * 2);
        ctx.fill();
      }

      if (c.name && projStars.length > 0 && projStars[0].up > 0.08) {
        ctx.fillStyle = `rgba(125, 211, 252, ${(0.82 * starAlpha).toFixed(3)})`;
        ctx.font = "9px sans-serif";
        ctx.fillText(c.name, projStars[0].x + 4, projStars[0].y - 3);
      }
    }
  }

  // 2. Planets: diurnal paths + animated positions
  if (sky.planets && cloudTrans > 0.14) {
    ctx.save();
    ctx.globalAlpha = Math.min(1.0, 0.22 + 0.85 * cloudTrans);
    for (const p of sky.planets) {
      const pCss = rgbFloatToCss(p.rgb, 240, 190, 110);
      drawDiurnalArc(
        ctx,
        (tFrac) => planetEquatorialAtDays(p, sky, baseDays + tFrac, epsRad),
        raNoon,
        latRad,
        cx,
        cy,
        radius,
        pCss,
        1.1,
        true
      );

      const pEq = planetEquatorialAtDays(p, sky, totalDays, epsRad);
      const pHoriz = equatorialToHorizontal(pEq, siderealNow, latRad);
      if (pHoriz[2] >= 0.0) {
        const pt = projectHorizontalToDome(pHoriz, cx, cy, radius);
        if (p.hasRings) {
          ctx.strokeStyle = "rgba(253, 230, 138, 0.85)";
          ctx.lineWidth = 1.1;
          ctx.beginPath();
          ctx.ellipse(pt.x, pt.y, 6.2, 2.4, -0.35, 0, Math.PI * 2);
          ctx.stroke();
        }
        ctx.fillStyle = pCss;
        ctx.strokeStyle = "#0f172a";
        ctx.lineWidth = 1.0;
        ctx.beginPath();
        ctx.arc(pt.x, pt.y, 3.3, 0, Math.PI * 2);
        ctx.fill();
        ctx.stroke();

        ctx.fillStyle = "#fde68a";
        ctx.font = "9.5px sans-serif";
        ctx.fillText(p.name, pt.x + 5, pt.y + 3);
      }
    }
    ctx.restore();
  }

  // 3. Moons: diurnal paths + animated phase discs
  if (sky.moons) {
    ctx.save();
    ctx.globalAlpha = Math.max(0.28, Math.min(1.0, 0.35 + 0.72 * cloudTrans));
    for (const m of sky.moons) {
      drawDiurnalArc(
        ctx,
        (tFrac) => moonEquatorialAtDays(m, baseDays + tFrac, epsRad),
        raNoon,
        latRad,
        cx,
        cy,
        radius,
        "rgba(203, 213, 225, 0.72)",
        1.25,
        true
      );

      const mEq = moonEquatorialAtDays(m, totalDays, epsRad);
      const mHoriz = equatorialToHorizontal(mEq, siderealNow, latRad);
      if (mHoriz[2] >= 0.0) {
        const pt = projectHorizontalToDome(mHoriz, cx, cy, radius);
        const rMoon = Math.max(3.8, 5.4 - m.index * 0.5);
        const cosElong =
          mEq[0] * sunEqNow[0] + mEq[1] * sunEqNow[1] + mEq[2] * sunEqNow[2];
        const illum = 0.5 * (1.0 - cosElong);

        ctx.fillStyle = "#1e293b";
        ctx.strokeStyle = "#94a3b8";
        ctx.lineWidth = 1.0;
        ctx.beginPath();
        ctx.arc(pt.x, pt.y, rMoon, 0, Math.PI * 2);
        ctx.fill();
        ctx.stroke();

        ctx.fillStyle = "#f1f5f9";
        ctx.beginPath();
        ctx.arc(pt.x, pt.y, rMoon * Math.max(0.2, illum), 0, Math.PI * 2);
        ctx.fill();

        ctx.fillStyle = "#e2e8f0";
        ctx.font = "9.5px sans-serif";
        ctx.fillText(m.name, pt.x + rMoon + 3, pt.y - 2);
      }
    }
    ctx.restore();
  }

  // 4. Sun: diurnal path arc + animated glowing Sun disc
  const sunCss = rgbFloatToCss(sky.sunRgb, 255, 228, 132);
  drawDiurnalArc(
    ctx,
    (tFrac) => sunEquatorialAtDays(sky, baseDays + tFrac, epsRad),
    raNoon,
    latRad,
    cx,
    cy,
    radius,
    `rgba(250, 204, 21, ${(0.45 + 0.47 * cloudTrans).toFixed(3)})`,
    1.85,
    false
  );

  if (sunHorizNow[2] >= -0.02) {
    const ptSun = projectHorizontalToDome(sunHorizNow, cx, cy, radius);
    const sunVisAlpha = Math.max(0.26, 0.32 + 0.68 * cloudTrans);
    ctx.save();
    ctx.globalAlpha = sunVisAlpha;
    const sunGlow = ctx.createRadialGradient(ptSun.x, ptSun.y, 1.5, ptSun.x, ptSun.y, 14.0);
    sunGlow.addColorStop(0.0, "#ffffff");
    sunGlow.addColorStop(0.35, sunCss);
    sunGlow.addColorStop(1.0, "rgba(250, 204, 21, 0.0)");
    ctx.fillStyle = sunGlow;
    ctx.beginPath();
    ctx.arc(ptSun.x, ptSun.y, 14.0, 0, Math.PI * 2);
    ctx.fill();

    ctx.fillStyle = sunCss;
    ctx.strokeStyle = "#fef08a";
    ctx.lineWidth = 1.2;
    ctx.beginPath();
    ctx.arc(ptSun.x, ptSun.y, 5.5, 0, Math.PI * 2);
    ctx.fill();
    ctx.stroke();

    ctx.fillStyle = "#fef08a";
    ctx.font = "bold 10px sans-serif";
    ctx.fillText(sky.sunName || "Sun", ptSun.x + 8, ptSun.y - 4);
    ctx.restore();
  }

  // 5. Live Cloud Decks & Precipitation Overlays on Sky Dome
  if (hasAtm && (cloudHigh > 0.04 || cloudLow > 0.04 || rainMmHr > 0.08 || snowMmHr > 0.04)) {
    const drift = (totalDays * 4.5 + (view.tileWeatherAnimPhase || 0.0) * 0.12) % 10.0;
    const wx = Math.sin(windAzRad);
    const wy = -Math.cos(windAzRad);
    const cloudBright = Math.round((78 + 165 * dayFactor) * (1.0 - 0.36 * stormDark));

    // High Cirrus wisps
    if (cloudHigh > 0.04) {
      ctx.save();
      ctx.strokeStyle = `rgba(${Math.min(255, cloudBright + 24)}, ${Math.min(255, cloudBright + 28)}, 250, ${(0.34 * cloudHigh).toFixed(3)})`;
      ctx.lineWidth = 2.6;
      for (let i = 0; i < 6; i++) {
        const seed = (i + 1) * 1.618 + (view.selectedTileIdx || 0) * 0.17;
        const baseU = (((Math.sin(seed * 3.1) * 0.5 + 0.5 + drift * 0.07) % 1.0) - 0.5) * radius * 1.5;
        const baseV = (Math.cos(seed * 2.3) * 0.62) * radius;
        const px = cx + baseU * wx - baseV * wy;
        const py = cy + baseU * wy + baseV * wx;
        ctx.beginPath();
        ctx.moveTo(px - wx * 18, py - wy * 18);
        ctx.quadraticCurveTo(px - wy * 5, py + wx * 5, px + wx * 20, py + wy * 20);
        ctx.stroke();
      }
      ctx.restore();
    }

    // Low Cumulus / Stratus puffs
    if (cloudLow > 0.04) {
      const nPuffs = Math.round(4 + 12 * cloudLow);
      const puffAlpha = Math.min(0.76, 0.18 + 0.54 * cloudLow);
      for (let i = 0; i < nPuffs; i++) {
        const s = (i + 1) * 2.39996 + (view.selectedTileIdx || 0) * 0.11;
        const u = ((((s * 0.37 + drift * 0.11) % 1.0) + 1.0) % 1.0 - 0.5) * radius * 1.75;
        const v = Math.sin(s * 1.9) * radius * 0.72;
        const px = cx + u * wx - v * wy;
        const py = cy + u * wy + v * wx;
        const rPuff = radius * (0.18 + 0.22 * cloudLow + 0.06 * (i % 3));
        const gCloud = ctx.createRadialGradient(px, py, rPuff * 0.15, px, py, rPuff);
        gCloud.addColorStop(0.0, `rgba(${cloudBright}, ${cloudBright + 4}, ${cloudBright + 12}, ${puffAlpha.toFixed(3)})`);
        gCloud.addColorStop(1.0, `rgba(${cloudBright}, ${cloudBright + 4}, ${cloudBright + 12}, 0)`);
        ctx.fillStyle = gCloud;
        ctx.beginPath();
        ctx.arc(px, py, rPuff, 0, Math.PI * 2);
        ctx.fill();
      }
    }

    // Precipitation streaks / flakes across dome
    const ph = view.tileWeatherAnimPhase || 0.0;
    if (rainMmHr > 0.08) {
      const nDrops = Math.min(34, Math.round(6 + rainMmHr * 3.5));
      ctx.strokeStyle = "rgba(186, 230, 253, 0.45)";
      ctx.lineWidth = 1.0;
      ctx.beginPath();
      for (let i = 0; i < nDrops; i++) {
        const u = ((((i * 0.618 + ph * 0.9) % 1.0) - 0.5) * 1.7) * radius;
        const v = ((((i * 0.382 + ph * 1.35) % 1.0) - 0.5) * 1.7) * radius;
        const rx = cx + u;
        const ry = cy + v;
        ctx.moveTo(rx, ry);
        ctx.lineTo(rx + wx * 6, ry + wy * 6 + 4);
      }
      ctx.stroke();
    } else if (snowMmHr > 0.04) {
      const nFlakes = Math.min(28, Math.round(6 + snowMmHr * 5.0));
      ctx.fillStyle = "rgba(248, 250, 252, 0.78)";
      for (let i = 0; i < nFlakes; i++) {
        const u = ((((i * 0.754 + ph * 0.28) % 1.0) - 0.5) * 1.7) * radius;
        const v = ((((i * 0.569 + ph * 0.41) % 1.0) - 0.5) * 1.7) * radius;
        ctx.beginPath();
        ctx.arc(cx + u, cy + v, 1.4, 0, Math.PI * 2);
        ctx.fill();
      }
    }
  }

  ctx.restore(); // End sky dome clip

  // Highlight active 100° vantage horizontal FOV sector on the sky dome horizon ring
  const activeNb = vantageCurrentNeighbor(view);
  if (activeNb) {
    const cAng = ((activeNb.azDeg - 90.0) * Math.PI) / 180.0;
    const halfFovRad = (50.0 * Math.PI) / 180.0;
    ctx.save();
    ctx.fillStyle = "rgba(56, 189, 248, 0.11)";
    ctx.beginPath();
    ctx.moveTo(cx, cy);
    ctx.arc(cx, cy, radius, cAng - halfFovRad, cAng + halfFovRad);
    ctx.closePath();
    ctx.fill();

    ctx.strokeStyle = "rgba(125, 211, 252, 0.88)";
    ctx.lineWidth = 2.2;
    ctx.beginPath();
    ctx.arc(cx, cy, radius, cAng - halfFovRad, cAng + halfFovRad);
    ctx.stroke();

    const tx = cx + Math.cos(cAng) * (radius + 3.5);
    const ty = cy + Math.sin(cAng) * (radius + 3.5);
    ctx.fillStyle = "#38bdf8";
    ctx.beginPath();
    ctx.arc(tx, ty, 2.4, 0, Math.PI * 2);
    ctx.fill();
    ctx.restore();
  }

  // Outer horizon ring & N / E / S / W cardinal labels
  ctx.strokeStyle = "#64748b";
  ctx.lineWidth = 1.6;
  ctx.beginPath();
  ctx.arc(cx, cy, radius, 0, Math.PI * 2);
  ctx.stroke();

  ctx.fillStyle = "#cbd5e1";
  ctx.font = "bold 10px sans-serif";
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";
  ctx.fillText("N", cx, cy - radius - 8);
  ctx.fillText("S", cx, cy + radius + 9);
  ctx.fillText("E", cx + radius + 9, cy);
  ctx.fillText("W", cx - radius - 10, cy);

  ctx.restore();
}

function vantageRockBaseRgb(rockName, rockKind, felsity) {
  const name = (rockName || "").toLowerCase();
  const kind = rockKind || "I";
  const f = typeof felsity === "number" ? Math.max(0.0, Math.min(1.0, felsity)) : 0.5;
  switch (name) {
    case "basalt":
      return f <= 0.35 ? [34, 62, 66] : [48, 56, 70];
    case "gabbro":
      return [38, 45, 58];
    case "andesite":
      return [82, 86, 108];
    case "diorite":
      return [74, 80, 98];
    case "dacite":
      return [116, 110, 132];
    case "granodiorite":
      return [148, 126, 138];
    case "rhyolite":
      return [184, 126, 122];
    case "granite":
      return [198, 140, 132];
    case "claystone":
      return [92, 96, 84];
    case "siltstone":
      return [118, 106, 86];
    case "sandstone":
      return [194, 146, 74];
    case "conglomerate":
      return [162, 106, 58];
    case "limestone":
      return [186, 184, 168];
    case "chalk":
      return [218, 215, 202];
    case "pelite":
      return [80, 88, 104];
    case "psammite":
      return [142, 116, 84];
    case "psephite":
      return [126, 90, 62];
    case "greenschist":
      return [58, 94, 72];
    case "blueschist":
      return [46, 76, 118];
    case "amphibolite":
      return [42, 66, 70];
    case "eclogite":
      return [40, 78, 60];
    case "hornfels":
      return [84, 62, 76];
    case "granulite":
      return [102, 72, 58];
    case "marble":
      return [228, 226, 216];
    default:
      if (kind === "S") return [144, 124, 86];
      if (kind === "M") return [82, 94, 112];
      return [
        Math.round(52 + 142 * f),
        Math.round(56 + 84 * f),
        Math.round(68 + 62 * f),
      ];
  }
}

// Continuous periodic world-azimuth ridge noise (periodic over 360° so rotating never shifts crags)
function vantageRidgeNoise(worldAzDeg, seed) {
  const rad = (worldAzDeg * Math.PI) / 180.0;
  const s = (seed || 0) * 0.713 + 1.37;
  const n1 = Math.sin(rad * 6.0 + s * 1.9) * 0.46;
  const n2 = Math.sin(rad * 13.0 - s * 3.1) * 0.28;
  const n3 = Math.cos(rad * 25.0 + s * 5.3) * 0.16;
  const n4 = (1.0 - Math.abs(Math.sin(rad * 19.0 + s * 2.7))) * 0.34 - 0.14;
  return n1 + n2 + n3 + n4;
}

export function paintTileVantageCanvas(view) {
  const canvas = document.getElementById("tile-vantage-canvas");
  if (!canvas || !view || !view.tileSky || !view.tileVantage) return;
  const sky = view.tileSky;
  const vantage = view.tileVantage;

  const rect = canvas.getBoundingClientRect();
  const cssW = Math.max(160, Math.round(rect.width || 200));
  const cssH = Math.max(180, Math.round(rect.height || 224));
  const dpr = window.devicePixelRatio || 1;
  const targetW = Math.round(cssW * dpr);
  const targetH = Math.round(cssH * dpr);
  if (canvas.width !== targetW || canvas.height !== targetH) {
    canvas.width = targetW;
    canvas.height = targetH;
  }

  const ctx = canvas.getContext("2d");
  if (!ctx) return;
  ctx.save();
  ctx.scale(dpr, dpr);

  // Proportional angular projection:
  // Horizontal FOV = 100° (-50°..+50° around active adjacent neighbor bearing)
  // Top edge = +90° (zenith straight overhead), bottom edge = -22° (foreground below horizon)
  const FOV_H_DEG = 100.0;
  const ALT_TOP_DEG = 90.0;
  const ALT_BOT_DEG = -22.0;
  const FOV_V_DEG = ALT_TOP_DEG - ALT_BOT_DEG;

  const curNb = vantageCurrentNeighbor(view);
  const viewAzDeg = curNb ? curNb.azDeg : 0.0;
  const altToY = (altDeg) => ((ALT_TOP_DEG - altDeg) / FOV_V_DEG) * cssH;
  const dAzToX = (dAzDeg) => (0.5 + dAzDeg / FOV_H_DEG) * cssW;
  const horizVecToAzAlt = (horiz) => {
    const east = horiz[0];
    const north = horiz[1];
    const up = Math.max(-1.0, Math.min(1.0, horiz[2]));
    const altDeg = (Math.asin(up) * 180.0) / Math.PI;
    const azDeg = (((Math.atan2(east, north) * 180.0) / Math.PI) % 360.0 + 360.0) % 360.0;
    return { azDeg, altDeg, up };
  };

  // Shared ephemeris & live weather/tide state with paintTileSkyCanvas
  const latRad = ((sky.latDeg || 0.0) * Math.PI) / 180.0;
  const epsRad = ((sky.tiltDeg || 23.5) * Math.PI) / 180.0;
  const { daysPerYear, year, day, tau, live } = resolveSkyTimeState(view, sky);
  const baseDays = year * daysPerYear + day;
  const totalDays = baseDays + tau;

  const cloudLow = live ? Math.max(0.0, Math.min(1.0, live.cloudLow || 0.0)) : 0.0;
  const cloudHigh = live ? Math.max(0.0, Math.min(1.0, live.cloudHigh || 0.0)) : 0.0;
  const stormDark = live ? Math.max(0.0, Math.min(1.0, live.stormDarkening || 0.0)) : 0.0;
  const rainMmHr = live ? Math.max(0.0, live.rainMmHr || 0.0) : 0.0;
  const snowMmHr = live ? Math.max(0.0, live.snowMmHr || 0.0) : 0.0;
  const snowDepthM = live ? Math.max(0.0, live.snowDepthM || 0.0) : 0.0;
  const windSpeedMps = live ? Math.max(0.0, live.windSpeedMps || 0.0) : 0.0;
  const windAzDeg = live ? live.windAzDeg || 0.0 : 0.0;
  const tideOffsetM = live && typeof live.tideOffsetM === "number" ? live.tideOffsetM : 0.0;
  const tidalRangeM = live ? Math.max(0.0, live.tidalRangeM || 0.0) : 0.0;
  const cloudTrans = (1.0 - 0.80 * cloudLow) * (1.0 - 0.34 * cloudHigh) * (1.0 - 0.45 * stormDark);

  const sunNoonEq = sunEquatorialAtDays(sky, baseDays + 0.5, epsRad);
  const raNoon = Math.atan2(sunNoonEq[1], sunNoonEq[0]);
  const siderealNow = raNoon + 2.0 * Math.PI * (tau - 0.5);

  const sunEqNow = sunEquatorialAtDays(sky, totalDays, epsRad);
  const sunHorizNow = equatorialToHorizontal(sunEqNow, siderealNow, latRad);
  const sunAA = horizVecToAzAlt(sunHorizNow);
  const sunAltDeg = sunAA.altDeg;
  const sunAzDeg = sunAA.azDeg;

  const hasAtm = sky.hasAtmosphere !== false;
  const isCoarse = !!vantage.obsIsCoarse;
  const dayFactor = Math.max(0.0, Math.min(1.0, (sunAltDeg + 6.0) / 24.0));
  const twiFactor = hasAtm
    ? Math.max(0.0, Math.min(1.0, 1.0 - Math.abs(sunAltDeg + 2.0) / 12.0))
    : 0.0;
  const zRgb = sky.skyZenithRgb || [0.16, 0.38, 0.82];
  const hRgb = sky.skyHorizonRgb || [0.48, 0.72, 0.96];
  const sRgb = sky.sunsetRgb || [0.95, 0.45, 0.22];
  const tRgb = sky.twilightRgb || [0.18, 0.22, 0.45];

  const overcastMix = hasAtm ? Math.min(0.88, cloudLow * 0.72 + stormDark * 0.42) : 0.0;
  const stormDim = 1.0 - stormDark * 0.48;
  const overcastDayLum = (0.14 + 0.56 * dayFactor) * stormDim;

  const mixCh = (nightV, dayV, twiV, greyBias) => {
    const clearV =
      nightV * (1.0 - dayFactor) + dayV * dayFactor * 0.86 + twiV * twiFactor * 0.46;
    const greyV = overcastDayLum * greyBias;
    return Math.round(
      Math.max(0, Math.min(255, (clearV * (1.0 - overcastMix) + greyV * overcastMix) * 255.0))
    );
  };
  const zenRgb = hasAtm
    ? [mixCh(0.02, zRgb[0], tRgb[0], 0.74), mixCh(0.04, zRgb[1], tRgb[1], 0.79), mixCh(0.11, zRgb[2], tRgb[2], 0.88)]
    : [4, 7, 15];
  const horRgb = hasAtm
    ? [mixCh(0.05, hRgb[0], sRgb[0], 0.80), mixCh(0.08, hRgb[1], sRgb[1], 0.84), mixCh(0.17, hRgb[2], sRgb[2], 0.91)]
    : [9, 13, 24];

  const yHor0 = altToY(0.0);

  // 1. Sky backdrop from +90° zenith (y = 0) down through horizon
  const skyGrad = ctx.createLinearGradient(0, 0, 0, Math.min(cssH, yHor0 + 18));
  skyGrad.addColorStop(0.0, `rgb(${zenRgb[0]},${zenRgb[1]},${zenRgb[2]})`);
  skyGrad.addColorStop(0.62, `rgb(${Math.round(zenRgb[0] * 0.45 + horRgb[0] * 0.55)},${Math.round(zenRgb[1] * 0.45 + horRgb[1] * 0.55)},${Math.round(zenRgb[2] * 0.45 + horRgb[2] * 0.55)})`);
  skyGrad.addColorStop(1.0, `rgb(${horRgb[0]},${horRgb[1]},${horRgb[2]})`);
  ctx.fillStyle = skyGrad;
  ctx.fillRect(0, 0, cssW, cssH);

  // Directional solar / sunset horizon scatter glow when facing toward the Sun
  const sunDAz = wrapDeltaAzDeg(sunAzDeg, viewAzDeg);
  if (hasAtm && sunAltDeg > -14.0 && Math.abs(sunDAz) < 88.0) {
    const sx = dAzToX(sunDAz);
    const sy = altToY(Math.max(-4.0, sunAltDeg));
    const glowStrength =
      Math.max(0.0, 1.0 - Math.abs(sunDAz) / 88.0) *
      (twiFactor * 0.58 + dayFactor * 0.24) *
      Math.max(0.18, cloudTrans);
    if (glowStrength > 0.01) {
      const gR = Math.round((sRgb[0] * 0.7 + (sky.sunRgb ? sky.sunRgb[0] : 1.0) * 0.3) * 255);
      const gG = Math.round((sRgb[1] * 0.7 + (sky.sunRgb ? sky.sunRgb[1] : 0.9) * 0.3) * 255);
      const gB = Math.round((sRgb[2] * 0.7 + (sky.sunRgb ? sky.sunRgb[2] : 0.6) * 0.3) * 255);
      const radGlow = ctx.createRadialGradient(sx, sy, 2.0, sx, sy, cssW * 0.78);
      radGlow.addColorStop(0.0, `rgba(${gR},${gG},${gB},${Math.min(0.65, glowStrength).toFixed(3)})`);
      radGlow.addColorStop(1.0, `rgba(${gR},${gG},${gB},0)`);
      ctx.fillStyle = radGlow;
      ctx.fillRect(0, 0, cssW, cssH);
    }
  }

  // 2. Stars & Constellations in horizontal (az, alt) space
  const starAlpha = (hasAtm ? Math.max(0.18, 1.0 - dayFactor * 0.76) : 0.94) * Math.max(0.06, cloudTrans);
  if (sky.constellations && starAlpha > 0.05) {
    for (const c of sky.constellations) {
      const projStars = (c.stars || []).map((st) => {
        const eq = raDecDegToEquatorial(st[0], st[1]);
        const horiz = equatorialToHorizontal(eq, siderealNow, latRad);
        const aa = horizVecToAzAlt(horiz);
        const dAz = wrapDeltaAzDeg(aa.azDeg, viewAzDeg);
        return {
          x: dAzToX(dAz),
          y: altToY(aa.altDeg),
          dAz,
          altDeg: aa.altDeg,
          mag: st[2] || 1.8,
        };
      });

      ctx.strokeStyle = `rgba(56, 189, 248, ${(0.55 * starAlpha).toFixed(3)})`;
      ctx.lineWidth = 1.0;
      for (const edge of c.edges || []) {
        const a = projStars[edge[0]];
        const b = projStars[edge[1]];
        if (
          a &&
          b &&
          (a.altDeg >= -4.0 || b.altDeg >= -4.0) &&
          Math.abs(a.dAz) <= 62.0 &&
          Math.abs(b.dAz) <= 62.0 &&
          Math.abs(a.dAz - b.dAz) < 92.0
        ) {
          ctx.beginPath();
          ctx.moveTo(a.x, a.y);
          ctx.lineTo(b.x, b.y);
          ctx.stroke();
        }
      }

      for (const ps of projStars) {
        if (ps.altDeg < -4.0 || Math.abs(ps.dAz) > 53.0) continue;
        const rStar = Math.max(1.1, 2.5 - 0.42 * ps.mag);
        ctx.fillStyle = `rgba(241, 245, 249, ${(0.95 * starAlpha).toFixed(3)})`;
        ctx.beginPath();
        ctx.arc(ps.x, ps.y, rStar, 0, Math.PI * 2);
        ctx.fill();
      }

      if (
        c.name &&
        projStars.length > 0 &&
        projStars[0].altDeg > 6.0 &&
        projStars[0].altDeg < 85.0 &&
        Math.abs(projStars[0].dAz) < 42.0
      ) {
        ctx.fillStyle = `rgba(125, 211, 252, ${(0.78 * starAlpha).toFixed(3)})`;
        ctx.font = "8.5px sans-serif";
        ctx.fillText(c.name, projStars[0].x + 4, projStars[0].y - 3);
      }
    }
  }

  // 3. Planets in (az, alt)
  if (sky.planets && cloudTrans > 0.14) {
    ctx.save();
    ctx.globalAlpha = Math.min(1.0, 0.22 + 0.85 * cloudTrans);
    for (const p of sky.planets) {
      const pEq = planetEquatorialAtDays(p, sky, totalDays, epsRad);
      const pHoriz = equatorialToHorizontal(pEq, siderealNow, latRad);
      const aa = horizVecToAzAlt(pHoriz);
      const dAz = wrapDeltaAzDeg(aa.azDeg, viewAzDeg);
      if (aa.altDeg >= -4.0 && Math.abs(dAz) <= 54.0) {
        const px = dAzToX(dAz);
        const py = altToY(aa.altDeg);
        const pCss = rgbFloatToCss(p.rgb, 240, 190, 110);
        if (p.hasRings) {
          ctx.strokeStyle = "rgba(253, 230, 138, 0.85)";
          ctx.lineWidth = 1.0;
          ctx.beginPath();
          ctx.ellipse(px, py, 5.8, 2.2, -0.35, 0, Math.PI * 2);
          ctx.stroke();
        }
        ctx.fillStyle = pCss;
        ctx.strokeStyle = "#0f172a";
        ctx.lineWidth = 0.9;
        ctx.beginPath();
        ctx.arc(px, py, 3.0, 0, Math.PI * 2);
        ctx.fill();
        ctx.stroke();
        if (Math.abs(dAz) < 44.0) {
          ctx.fillStyle = "#fde68a";
          ctx.font = "8.5px sans-serif";
          ctx.fillText(p.name, px + 4, py + 3);
        }
      }
    }
    ctx.restore();
  }

  // 4. Moons in (az, alt) + track brightest moon illumination for nocturnal landscape light
  let nightMoonBoost = 0.0;
  let primaryMoonWaterRefl = null;
  if (sky.moons) {
    ctx.save();
    ctx.globalAlpha = Math.max(0.28, Math.min(1.0, 0.35 + 0.72 * cloudTrans));
    for (const m of sky.moons) {
      const mEq = moonEquatorialAtDays(m, totalDays, epsRad);
      const mHoriz = equatorialToHorizontal(mEq, siderealNow, latRad);
      const aa = horizVecToAzAlt(mHoriz);
      const dAz = wrapDeltaAzDeg(aa.azDeg, viewAzDeg);
      const cosElong = mEq[0] * sunEqNow[0] + mEq[1] * sunEqNow[1] + mEq[2] * sunEqNow[2];
      const illum = 0.5 * (1.0 - cosElong);
      if (aa.altDeg > 0.0) {
        nightMoonBoost = Math.max(nightMoonBoost, illum * Math.min(1.0, aa.altDeg / 25.0) * cloudTrans);
        if (!primaryMoonWaterRefl && Math.abs(dAz) <= 48.0 && illum > 0.22 && cloudTrans > 0.25) {
          primaryMoonWaterRefl = { dAz, altDeg: aa.altDeg, illum };
        }
      }
      if (aa.altDeg >= -4.0 && Math.abs(dAz) <= 55.0) {
        const mx = dAzToX(dAz);
        const my = altToY(aa.altDeg);
        const rMoon = Math.max(3.6, 5.2 - m.index * 0.5);
        ctx.fillStyle = "#1e293b";
        ctx.strokeStyle = "#94a3b8";
        ctx.lineWidth = 0.9;
        ctx.beginPath();
        ctx.arc(mx, my, rMoon, 0, Math.PI * 2);
        ctx.fill();
        ctx.stroke();

        ctx.fillStyle = "#f1f5f9";
        ctx.beginPath();
        ctx.arc(mx, my, rMoon * Math.max(0.2, illum), 0, Math.PI * 2);
        ctx.fill();

        if (Math.abs(dAz) < 44.0) {
          ctx.fillStyle = "#e2e8f0";
          ctx.font = "8.5px sans-serif";
          ctx.fillText(m.name, mx + rMoon + 3, my - 2);
        }
      }
    }
    ctx.restore();
  }

  // 5. Sun in (az, alt) (drawn BEFORE clouds & mountains so it sets cleanly behind peaks)
  const sunCss = rgbFloatToCss(sky.sunRgb, 255, 228, 132);
  if (sunAltDeg >= -5.0 && Math.abs(sunDAz) <= 58.0) {
    const sx = dAzToX(sunDAz);
    const sy = altToY(sunAltDeg);
    ctx.save();
    ctx.globalAlpha = Math.max(0.24, 0.30 + 0.70 * cloudTrans);
    const sunGlow = ctx.createRadialGradient(sx, sy, 1.5, sx, sy, 15.0);
    sunGlow.addColorStop(0.0, "#ffffff");
    sunGlow.addColorStop(0.35, sunCss);
    sunGlow.addColorStop(1.0, "rgba(250, 204, 21, 0.0)");
    ctx.fillStyle = sunGlow;
    ctx.beginPath();
    ctx.arc(sx, sy, 15.0, 0, Math.PI * 2);
    ctx.fill();

    ctx.fillStyle = sunCss;
    ctx.strokeStyle = "#fef08a";
    ctx.lineWidth = 1.1;
    ctx.beginPath();
    ctx.arc(sx, sy, 5.2, 0, Math.PI * 2);
    ctx.fill();
    ctx.stroke();

    if (Math.abs(sunDAz) < 43.0) {
      ctx.fillStyle = "#fef08a";
      ctx.font = "bold 9px sans-serif";
      ctx.fillText(sky.sunName || "Sun", sx + 7, sy - 4);
    }
    ctx.restore();
  }

  // 5b. Live High Cirrus & Mid/Low Cloud Decks in the Sky (before distant mountains so high peaks pierce low clouds!)
  if (hasAtm && (cloudHigh > 0.04 || cloudLow > 0.04)) {
    const cloudBaseLum = Math.round((68 + 168 * dayFactor) * (1.0 - 0.42 * stormDark));
    const cloudShadowLum = Math.round(cloudBaseLum * (0.62 - 0.18 * stormDark));
    const windRelAz = wrapDeltaAzDeg(windAzDeg, viewAzDeg);
    const driftDeg = (totalDays * 48.0 + (view.tileWeatherAnimPhase || 0.0) * 1.4) * (windRelAz >= 0 ? 1 : -1);

    if (cloudHigh > 0.04) {
      ctx.save();
      ctx.strokeStyle = `rgba(${Math.min(255, cloudBaseLum + 26)}, ${Math.min(255, cloudBaseLum + 30)}, 252, ${(0.36 * cloudHigh).toFixed(3)})`;
      ctx.lineWidth = 2.4;
      for (let i = 0; i < 7; i++) {
        const wAz = ((i * 51.4 + driftDeg * 0.4) % 360.0 + 360.0) % 360.0;
        const dAz = wrapDeltaAzDeg(wAz, viewAzDeg);
        if (Math.abs(dAz) > 65.0) continue;
        const alt = 44.0 + (i % 4) * 9.5;
        const x0 = dAzToX(dAz - 14.0);
        const x1 = dAzToX(dAz + 14.0);
        const y0 = altToY(alt);
        ctx.beginPath();
        ctx.moveTo(x0, y0 + 2);
        ctx.quadraticCurveTo((x0 + x1) * 0.5, y0 - 3, x1, y0 + 1);
        ctx.stroke();
      }
      ctx.restore();
    }

    if (cloudLow > 0.04) {
      const nBanks = 10;
      const bankAlpha = Math.min(0.84, 0.24 + 0.58 * cloudLow);
      for (let i = 0; i < nBanks; i++) {
        const wAz = ((i * 36.0 + driftDeg) % 360.0 + 360.0) % 360.0;
        const dAz = wrapDeltaAzDeg(wAz, viewAzDeg);
        if (Math.abs(dAz) > 68.0) continue;
        const altBase = 10.0 + (i % 3) * 7.5 - 3.5 * stormDark;
        const puffW = cssW * (0.20 + 0.22 * cloudLow);
        const puffH = 12.0 + 18.0 * cloudLow + (i % 2) * 5.0;
        const cxP = dAzToX(dAz);
        const cyP = altToY(altBase + 6.0);
        const cg = ctx.createLinearGradient(cxP, cyP - puffH, cxP, cyP + puffH * 0.55);
        cg.addColorStop(0.0, `rgba(${Math.min(255, cloudBaseLum + 16)}, ${Math.min(255, cloudBaseLum + 18)}, ${Math.min(255, cloudBaseLum + 24)}, ${bankAlpha.toFixed(3)})`);
        cg.addColorStop(0.75, `rgba(${cloudShadowLum}, ${cloudShadowLum + 4}, ${cloudShadowLum + 12}, ${(bankAlpha * 0.92).toFixed(3)})`);
        cg.addColorStop(1.0, `rgba(${cloudShadowLum}, ${cloudShadowLum + 4}, ${cloudShadowLum + 12}, 0)`);
        ctx.fillStyle = cg;
        ctx.beginPath();
        ctx.ellipse(cxP, cyP, puffW, puffH, 0, 0, Math.PI * 2);
        ctx.fill();
      }
    }
  }

  // =========================================================================
  // 6. Accurate Horizon, Sea, Distant Peaks, Adjacent Neighbors & Foreground
  // =========================================================================
  const R = Math.max(500.0, vantage.planetRadiusKm || 6371.0);
  const ring1Km = Math.max(8.0, vantage.ring1DistKm || 100.0);
  const obsIsLand = !hasAtm || !!vantage.obsIsLand;
  const obsGroundKm = obsIsLand ? Math.max(0.0, vantage.obsElevKm || 0.0) : 0.0;
  const hObsKm = Math.max(0.002, obsGroundKm + 0.015 - tideOffsetM * 0.001);
  const pfRgb = vantage.plantFoliageRgb || [0.18, 0.52, 0.20];
  const phRgb = vantage.plantHighlightRgb || [0.34, 0.68, 0.28];
  const plantCssRgb = [
    Math.round(Math.max(0, Math.min(255, (pfRgb[0] * 0.72 + phRgb[0] * 0.28) * 255))),
    Math.round(Math.max(0, Math.min(255, (pfRgb[1] * 0.72 + phRgb[1] * 0.28) * 255))),
    Math.round(Math.max(0, Math.min(255, (pfRgb[2] * 0.72 + phRgb[2] * 0.28) * 255))),
  ];
  const absLat = Math.abs(sky.latDeg || 0.0);
  const snowlineKm = Math.max(0.8, 4.7 - 0.048 * absLat);
  const liveSnowCover = hasAtm ? Math.max(0.0, Math.min(1.0, snowDepthM / 0.12)) : 0.0;

  // Visual tide vertical shift in degrees for coastal shoreline & intertidal bands
  const tideVisualDeg = Math.max(-2.6, Math.min(3.2, tideOffsetM * 0.45));

  // Spherical sea-horizon depression angle from observer's elevation
  const seaDipDeg = -Math.min(
    7.8,
    Math.sqrt((2.0 * Math.max(0.002, hObsKm)) / R) * (180.0 / Math.PI) * 2.8
  );
  const ySeaHor = altToY(seaDipDeg);

  // Ambient + direct illumination factor for terrain at world azimuth `az`
  const rawBaseLight = hasAtm
    ? 0.16 + 0.14 * nightMoonBoost + 0.78 * dayFactor + 0.14 * twiFactor
    : 0.08 + 0.12 * nightMoonBoost + 0.90 * dayFactor;
  const baseLight = rawBaseLight * (1.0 - 0.36 * stormDark - 0.18 * cloudLow);
  const precipHazeBoost = Math.min(0.42, rainMmHr * 0.035 + snowMmHr * 0.06);

  const sunTintRgb = [
    (sky.sunRgb ? sky.sunRgb[0] : 1.0) * (1.0 - twiFactor * 0.45) + sRgb[0] * twiFactor * 0.45,
    (sky.sunRgb ? sky.sunRgb[1] : 0.96) * (1.0 - twiFactor * 0.45) + sRgb[1] * twiFactor * 0.45,
    (sky.sunRgb ? sky.sunRgb[2] : 0.88) * (1.0 - twiFactor * 0.45) + sRgb[2] * twiFactor * 0.45,
  ];
  const shadeTerrainRgb = (rgb, worldAz, slopeContrast, hazeFrac) => {
    const azDiffRad = (wrapDeltaAzDeg(worldAz, sunAzDeg) * Math.PI) / 180.0;
    const direct =
      dayFactor > 0.02
        ? Math.max(-0.35, Math.cos(azDiffRad)) *
          slopeContrast *
          Math.min(1.0, (sunAltDeg + 6.0) / 30.0) *
          cloudTrans
        : 0.0;
    const lit = Math.max(hasAtm ? 0.10 : 0.06, Math.min(1.32, baseLight + direct));
    const rLit = rgb[0] * lit * (0.86 + 0.18 * sunTintRgb[0]);
    const gLit = rgb[1] * lit * (0.86 + 0.18 * sunTintRgb[1]);
    const bLit = rgb[2] * lit * (0.88 + 0.16 * sunTintRgb[2]);
    const hz = hasAtm ? Math.max(0.0, Math.min(0.88, hazeFrac + precipHazeBoost)) : 0.0;
    const rOut = Math.round(Math.max(0, Math.min(255, rLit * (1.0 - hz) + horRgb[0] * hz)));
    const gOut = Math.round(Math.max(0, Math.min(255, gLit * (1.0 - hz) + horRgb[1] * hz)));
    const bOut = Math.round(Math.max(0, Math.min(255, bLit * (1.0 - hz) + horRgb[2] * hz)));
    return `rgb(${rOut},${gOut},${bOut})`;
  };

  const blendTileSurfaceRgb = (rockRgb, vegFrac, mountNorm, koeppen, elevKm) => {
    if (hasAtm && (koeppen === 12 || koeppen === 13 || elevKm > snowlineKm + 0.65)) {
      return [228, 236, 246];
    }
    const effVeg = hasAtm ? vegFrac : 0.0;
    const vegCover = Math.max(0.0, Math.min(0.88, effVeg * (0.88 - 0.36 * mountNorm)));
    let r = rockRgb[0] * (1.0 - vegCover) + plantCssRgb[0] * vegCover;
    let g = rockRgb[1] * (1.0 - vegCover) + plantCssRgb[1] * vegCover;
    let b = rockRgb[2] * (1.0 - vegCover) + plantCssRgb[2] * vegCover;
    if (hasAtm && (koeppen === 4 || koeppen === 5)) {
      const aridW = (1.0 - vegCover) * 0.28;
      r = r * (1.0 - aridW) + 208 * aridW;
      g = g * (1.0 - aridW) + 172 * aridW;
      b = b * (1.0 - aridW) + 112 * aridW;
    }
    if (liveSnowCover > 0.02) {
      const sw = Math.min(0.95, liveSnowCover * (0.78 + 0.22 * mountNorm));
      r = r * (1.0 - sw) + 236 * sw;
      g = g * (1.0 - sw) + 243 * sw;
      b = b * (1.0 - sw) + 251 * sw;
    }
    return [r, g, b];
  };

  // 6a. Base Sea / Ocean horizon plane
  if (hasAtm && (!isCoarse || !obsIsLand)) {
    const seaSkyR = Math.round((0.04 + 0.42 * zRgb[0]) * 255 * Math.max(0.16, baseLight));
    const seaSkyG = Math.round((0.09 + 0.50 * zRgb[1]) * 255 * Math.max(0.16, baseLight));
    const seaSkyB = Math.round((0.20 + 0.64 * zRgb[2]) * 255 * Math.max(0.18, baseLight));
    const seaHorR = Math.round(seaSkyR * 0.55 + horRgb[0] * 0.45);
    const seaHorG = Math.round(seaSkyG * 0.55 + horRgb[1] * 0.45);
    const seaHorB = Math.round(seaSkyB * 0.55 + horRgb[2] * 0.45);
    const seaGrad = ctx.createLinearGradient(0, ySeaHor, 0, cssH);
    seaGrad.addColorStop(0.0, `rgb(${seaHorR},${seaHorG},${seaHorB})`);
    seaGrad.addColorStop(0.45, `rgb(${seaSkyR},${seaSkyG},${seaSkyB})`);
    seaGrad.addColorStop(
      1.0,
      `rgb(${Math.round(seaSkyR * 0.68)},${Math.round(seaSkyG * 0.72)},${Math.round(seaSkyB * 0.78)})`
    );
    ctx.fillStyle = seaGrad;
    ctx.fillRect(0, ySeaHor, cssW, cssH - ySeaHor);

    // Ocean wave shimmer & Beaufort wind-driven whitecaps
    const galeFactor = Math.max(0.0, Math.min(1.0, (windSpeedMps - 4.0) / 18.0));
    const nWaveRows = 7 + Math.round(galeFactor * 6);
    const animPh = view.tileWeatherAnimPhase || 0.0;
    ctx.strokeStyle = `rgba(206, 238, 255, ${(0.11 + 0.14 * dayFactor + 0.34 * galeFactor).toFixed(3)})`;
    ctx.lineWidth = 0.75 + 0.65 * galeFactor;
    for (let wIdx = 0; wIdx < nWaveRows; wIdx++) {
      const tW = Math.pow((wIdx + 0.5) / (nWaveRows + 0.2), 1.65);
      const wy = ySeaHor + 2.0 + tW * (cssH - ySeaHor - 4.0);
      ctx.beginPath();
      for (let seg = 0; seg < 6; seg++) {
        const ph =
          ((viewAzDeg * 0.04 + seg * 0.19 + wIdx * 0.31 + animPh * 0.06 * (0.4 + galeFactor)) % 1.0 +
            1.0) %
          1.0;
        const wx0 = ph * cssW;
        const wLen = (7.0 + tW * 16.0) * (1.0 - 0.25 * galeFactor);
        ctx.moveTo(wx0, wy);
        ctx.lineTo(Math.min(cssW, wx0 + wLen), wy - galeFactor * 1.1);
      }
      ctx.stroke();
    }
    if (sunAltDeg > -2.0 && Math.abs(sunDAz) < 52.0 && cloudTrans > 0.2) {
      const sx = dAzToX(sunDAz);
      for (let rIdx = 0; rIdx < 9; rIdx++) {
        const tr = rIdx / 8.0;
        const ry = ySeaHor + 1.5 + tr * tr * (cssH - ySeaHor - 3.0);
        const rw = 2.5 + tr * 11.0;
        ctx.fillStyle = `rgba(254, 240, 138, ${(0.34 * cloudTrans * (1.0 - tr * 0.55)).toFixed(3)})`;
        ctx.fillRect(sx - rw * 0.5, ry, rw, 1.3);
      }
    } else if (primaryMoonWaterRefl) {
      const mx = dAzToX(primaryMoonWaterRefl.dAz);
      for (let rIdx = 0; rIdx < 7; rIdx++) {
        const tr = rIdx / 6.0;
        const ry = ySeaHor + 2.0 + tr * tr * (cssH - ySeaHor - 4.0);
        const rw = 2.0 + tr * 8.0;
        ctx.fillStyle = `rgba(226, 232, 240, ${(0.24 * primaryMoonWaterRefl.illum * (1.0 - tr * 0.5)).toFixed(3)})`;
        ctx.fillRect(mx - rw * 0.5, ry, rw, 1.1);
      }
    }
  }

  if (isCoarse && obsIsLand) {
    const obsRockRgb = vantageRockBaseRgb(
      vantage.obsRockName,
      vantage.obsRockKind,
      vantage.obsFelsity
    );
    const obsMount = Math.max(0.0, Math.min(1.0, vantage.obsMountainosityNorm || 0.0));
    const obsElev = vantage.obsElevKm || 0.0;
    const obsKoeppen = hasAtm ? vantage.obsKoeppen || 0 : 0;
    const obsSurfRgb = blendTileSurfaceRgb(
      obsRockRgb,
      vantage.obsVegFrac || 0.0,
      obsMount,
      obsKoeppen,
      obsElev
    );
    const seedBase = (view.selectedTileIdx || 0) + 19;
    const N_COLS = 96;

    const farPts = [];
    for (let i = 0; i <= N_COLS; i++) {
      const dAz = -50.0 + (100.0 * i) / N_COLS;
      const wAz = ((viewAzDeg + dAz) % 360.0 + 360.0) % 360.0;
      const crag = vantageRidgeNoise(wAz, seedBase);
      const crag2 = vantageRidgeNoise(wAz * 2.0 + 41.0, seedBase + 13);
      const reliefKm =
        obsMount * (0.45 + 2.35 * Math.max(0.0, 0.52 + crag + 0.25 * crag2)) +
        0.035 * (0.5 + crag);
      const altDeg =
        seaDipDeg * (1.0 - 0.5 * obsMount) +
        (Math.atan2(reliefKm, 12.5) * 180.0) / Math.PI;
      farPts.push({
        x: dAzToX(dAz),
        y: altToY(altDeg),
        wAz,
        peakKm: Math.max(0.0, obsElev) + reliefKm,
      });
    }
    for (let j = 0; j < farPts.length - 1; j++) {
      const a = farPts[j];
      const b = farPts[j + 1];
      const slope = (a.y - b.y) * 0.16;
      ctx.fillStyle = shadeTerrainRgb(
        obsSurfRgb,
        a.wAz - slope * 20.0,
        0.24 + 0.32 * obsMount,
        0.26
      );
      ctx.beginPath();
      ctx.moveTo(a.x - 0.4, cssH);
      ctx.lineTo(a.x - 0.4, a.y);
      ctx.lineTo(b.x + 0.4, b.y);
      ctx.lineTo(b.x + 0.4, cssH);
      ctx.closePath();
      ctx.fill();

      if (obsMount > 0.16) {
        const rockFrac = Math.min(0.58, 0.20 + 0.48 * obsMount);
        const ayMid = a.y + (altToY(-4.5) - a.y) * rockFrac;
        const byMid = b.y + (altToY(-4.5) - b.y) * rockFrac;
        ctx.fillStyle = shadeTerrainRgb(obsRockRgb, a.wAz - slope * 26.0, 0.38, 0.20);
        ctx.beginPath();
        ctx.moveTo(a.x - 0.3, ayMid);
        ctx.lineTo(a.x - 0.3, a.y);
        ctx.lineTo(b.x + 0.3, b.y);
        ctx.lineTo(b.x + 0.3, byMid);
        ctx.closePath();
        ctx.fill();
      }

      if (hasAtm && (obsKoeppen === 12 || obsKoeppen === 13 || a.peakKm > snowlineKm || liveSnowCover > 0.25)) {
        const snowFrac =
          obsKoeppen === 12 || obsKoeppen === 13
            ? 0.65
            : Math.max(liveSnowCover * 0.55, Math.min(0.42, (a.peakKm - snowlineKm) * 0.24));
        const ayS = a.y + (altToY(-4.5) - a.y) * snowFrac;
        const byS = b.y + (altToY(-4.5) - b.y) * snowFrac;
        ctx.fillStyle = shadeTerrainRgb([238, 244, 252], a.wAz, 0.24, 0.16);
        ctx.beginPath();
        ctx.moveTo(a.x - 0.3, ayS);
        ctx.lineTo(a.x - 0.3, a.y);
        ctx.lineTo(b.x + 0.3, b.y);
        ctx.lineTo(b.x + 0.3, byS);
        ctx.closePath();
        ctx.fill();
      }
    }
    ctx.strokeStyle = "rgba(15, 23, 42, 0.48)";
    ctx.lineWidth = 0.85;
    ctx.beginPath();
    ctx.moveTo(farPts[0].x, farPts[0].y);
    for (let j = 1; j < farPts.length; j++) ctx.lineTo(farPts[j].x, farPts[j].y);
    ctx.stroke();

    const midSteps = 64;
    ctx.fillStyle = shadeTerrainRgb(obsSurfRgb, viewAzDeg + 12.0, 0.26, 0.10);
    ctx.beginPath();
    ctx.moveTo(0, cssH);
    for (let k = 0; k <= midSteps; k++) {
      const dAz = -50.0 + (100.0 * k) / midSteps;
      const wAz = ((viewAzDeg + dAz) % 360.0 + 360.0) % 360.0;
      const und = vantageRidgeNoise(wAz + 85.0, seedBase + 47);
      const midAlt = -2.8 + (0.35 + 4.8 * obsMount) * (0.25 + und);
      ctx.lineTo(dAzToX(dAz), altToY(midAlt));
    }
    ctx.lineTo(cssW, cssH);
    ctx.closePath();
    ctx.fill();
  }

  // 6b. Distant & Mid-ground Land Tiles (2..4 rings away)
  const samples = !isCoarse && Array.isArray(vantage.samples) ? vantage.samples : [];
  for (const smp of samples) {
    if (smp.isAdjacent || !smp.isLand) continue;
    const dAzCenter = wrapDeltaAzDeg(smp.azDeg, viewAzDeg);
    const spanDeg = Math.max(4.5, (smp.halfWidthDeg || 10.0) * 1.38);
    if (Math.abs(dAzCenter) > 50.0 + spanDeg) continue;

    const curvDropKm = (smp.distKm * smp.distKm) / (2.0 * R);
    const rRatio = Math.max(1.2, smp.distKm / ring1Km);
    const dVisKm = 14.5 * rRatio;
    const peakPotKm =
      smp.elevKm * (1.0 + 0.68 * smp.mountainosityNorm) + 0.45 * smp.mountainosityNorm;
    const maxAltEst =
      (Math.atan2(peakPotKm - hObsKm - curvDropKm * 0.58, dVisKm) * 180.0) / Math.PI;
    if (maxAltEst < seaDipDeg - 0.8) continue;

    const rockRgb = vantageRockBaseRgb(smp.rockName, smp.rockKind, smp.felsity);
    const surfRgb = blendTileSurfaceRgb(
      rockRgb,
      smp.vegFrac,
      smp.mountainosityNorm,
      smp.koeppen,
      smp.elevKm
    );
    const haze = Math.min(0.78, 1.0 - Math.exp(-smp.distKm / (ring1Km * 2.85)));
    const fillCss = shadeTerrainRgb(surfRgb, smp.azDeg, 0.24 + 0.28 * smp.mountainosityNorm, haze);

    const steps = 18;
    const yBase = Math.min(cssH, ySeaHor + 3.0);
    ctx.beginPath();
    ctx.moveTo(dAzToX(dAzCenter - spanDeg), yBase);
    let anyAbove = false;
    const ridgePts = [];
    for (let k = 0; k <= steps; k++) {
      const u = (k / steps) * 2.0 - 1.0;
      const env = Math.pow(Math.max(0.0, 1.0 - u * u), 0.72 - 0.22 * smp.mountainosityNorm);
      const wAz = smp.azDeg + u * spanDeg;
      const crag = vantageRidgeNoise(wAz, smp.displayIdx + 17);
      const localH =
        env *
        Math.max(
          0.02,
          smp.elevKm * (1.0 + 0.68 * smp.mountainosityNorm * (0.45 + crag)) +
            0.38 * smp.mountainosityNorm * Math.max(0.0, crag + 0.35)
        );
      const altDeg =
        (Math.atan2(localH - hObsKm * env - curvDropKm * 0.58, dVisKm) * 180.0) / Math.PI;
      const px = dAzToX(dAzCenter + u * spanDeg);
      const py = Math.min(yBase, altToY(altDeg));
      if (py < ySeaHor - 0.4) anyAbove = true;
      ridgePts.push({ px, py, localH });
      ctx.lineTo(px, py);
    }
    ctx.lineTo(dAzToX(dAzCenter + spanDeg), yBase);
    ctx.closePath();
    if (anyAbove) {
      ctx.fillStyle = fillCss;
      ctx.fill();

      if (smp.koeppen === 12 || smp.koeppen === 13 || peakPotKm > snowlineKm || liveSnowCover > 0.3) {
        const capCss = shadeTerrainRgb([236, 242, 250], smp.azDeg, 0.22, haze * 0.85);
        ctx.strokeStyle = capCss;
        ctx.lineWidth = 1.2;
        ctx.beginPath();
        let pen = false;
        for (const rp of ridgePts) {
          if ((rp.localH > snowlineKm * 0.92 || liveSnowCover > 0.45) && rp.py < ySeaHor - 0.5) {
            if (!pen) {
              ctx.moveTo(rp.px, rp.py + 0.7);
              pen = true;
            } else {
              ctx.lineTo(rp.px, rp.py + 0.7);
            }
          } else {
            pen = false;
          }
        }
        ctx.stroke();
      }
    }
  }

  // 6c. 1-Ring Adjacent Neighbor Tiles (continuous 360° horizon & coastal relief + live tidal waterline!)
  const adjSamples = samples
    .filter((s) => s.isAdjacent)
    .slice()
    .sort((a, b) => a.azDeg - b.azDeg);

  let anyWaterNeighborInFov = false;
  if (adjSamples.length > 0) {
    const N_COLS = 110;
    const dVis1Km = 13.5;
    const colData = [];

    for (let i = 0; i <= N_COLS; i++) {
      const dAz = -50.0 + (100.0 * i) / N_COLS;
      const worldAz = ((viewAzDeg + dAz) % 360.0 + 360.0) % 360.0;

      let wSum = 0.0;
      let landW = 0.0;
      let elevSum = 0.0;
      let mountSum = 0.0;
      let vegSum = 0.0;
      let snowW = 0.0;
      let riverW = 0.0;
      let rRock = 0.0;
      let gRock = 0.0;
      let bRock = 0.0;
      let rSurf = 0.0;
      let gSurf = 0.0;
      let bSurf = 0.0;

      for (const adj of adjSamples) {
        const ang = Math.abs(wrapDeltaAzDeg(worldAz, adj.azDeg));
        const halfSpan = Math.max(26.0, adj.halfWidthDeg || 32.0);
        if (ang > halfSpan * 1.65) continue;
        const t = ang / (halfSpan * 0.78);
        const w = Math.exp(-t * t * 1.45);
        wSum += w;
        if (adj.isLand) {
          landW += w;
          const rk = vantageRockBaseRgb(adj.rockName, adj.rockKind, adj.felsity);
          const sf = blendTileSurfaceRgb(
            rk,
            adj.vegFrac,
            adj.mountainosityNorm,
            adj.koeppen,
            adj.elevKm
          );
          elevSum += w * Math.max(0.02, adj.elevKm);
          mountSum += w * adj.mountainosityNorm;
          vegSum += w * adj.vegFrac;
          if (adj.koeppen === 12 || adj.koeppen === 13) snowW += w;
          if (adj.isRiver) riverW += w;
          rRock += w * rk[0];
          gRock += w * rk[1];
          bRock += w * rk[2];
          rSurf += w * sf[0];
          gSurf += w * sf[1];
          bSurf += w * sf[2];
        }
      }

      const landFrac = wSum > 1e-5 ? landW / wSum : 0.0;
      if (landFrac < 0.85) {
        anyWaterNeighborInFov = true;
      }
      if (landFrac > 0.14 && landW > 1e-5) {
        const meanElev = elevSum / landW;
        const meanMount = mountSum / landW;
        const meanVeg = vegSum / landW;
        const snowFrac = snowW / landW;
        const riverFrac = riverW / landW;
        const coastTaper = Math.min(1.0, (landFrac - 0.14) / 0.42);
        const crag = vantageRidgeNoise(worldAz, (view.selectedTileIdx || 0) + 5);
        const cragFine = vantageRidgeNoise(worldAz * 2.0 + 37.0, (view.selectedTileIdx || 0) + 29);

        const crestKm =
          coastTaper *
          (meanElev * (1.0 + 0.72 * meanMount * (0.38 + crag)) +
            0.46 * meanMount * Math.max(0.0, crag + 0.35 + 0.25 * cragFine) +
            0.04 * (0.5 + crag));
        const altCrestDeg = (Math.atan2(crestKm - hObsKm * coastTaper, dVis1Km) * 180.0) / Math.PI;
        // Coastal shoreline rises/falls with live dynamic tide (`tideVisualDeg`)
        const coastalTideShift = (1.0 - coastTaper * 0.55) * tideVisualDeg * 0.55;
        const shoreAltDeg =
          (obsIsLand
            ? -6.8 * coastTaper + (seaDipDeg - 1.4) * (1.0 - coastTaper)
            : seaDipDeg - 0.35 * coastTaper) + coastalTideShift;

        colData.push({
          hasLand: true,
          x: dAzToX(dAz),
          worldAz,
          crestKm,
          meanMount,
          meanVeg,
          snowFrac,
          riverFrac,
          coastTaper,
          yCrest: altToY(altCrestDeg),
          yBase: altToY(shoreAltDeg),
          rockRgb: [rRock / landW, gRock / landW, bRock / landW],
          surfRgb: [rSurf / landW, gSurf / landW, bSurf / landW],
        });
      } else {
        colData.push({
          hasLand: false,
          x: dAzToX(dAz),
          worldAz,
        });
      }
    }

    let idx = 0;
    while (idx < colData.length) {
      if (!colData[idx].hasLand) {
        idx++;
        continue;
      }
      let endIdx = idx;
      while (endIdx + 1 < colData.length && colData[endIdx + 1].hasLand) {
        endIdx++;
      }
      const seg = colData.slice(idx, endIdx + 1);
      if (seg.length >= 2) {
        for (let j = 0; j < seg.length - 1; j++) {
          const a = seg[j];
          const b = seg[j + 1];
          const slopeDeg = (a.yCrest - b.yCrest) * 0.14;
          const colCss = shadeTerrainRgb(
            a.surfRgb,
            a.worldAz - slopeDeg * 18.0,
            0.28 + 0.30 * a.meanMount,
            0.12
          );
          ctx.fillStyle = colCss;
          ctx.beginPath();
          ctx.moveTo(a.x - 0.4, a.yBase);
          ctx.lineTo(a.x - 0.4, a.yCrest);
          ctx.lineTo(b.x + 0.4, b.yCrest);
          ctx.lineTo(b.x + 0.4, b.yBase);
          ctx.closePath();
          ctx.fill();

          if (a.meanMount > 0.16) {
            const rockFrac = Math.min(0.62, 0.22 + 0.52 * a.meanMount);
            const ayMid = a.yCrest + (a.yBase - a.yCrest) * rockFrac;
            const byMid = b.yCrest + (b.yBase - b.yCrest) * rockFrac;
            ctx.fillStyle = shadeTerrainRgb(
              a.rockRgb,
              a.worldAz - slopeDeg * 24.0,
              0.38,
              0.10
            );
            ctx.beginPath();
            ctx.moveTo(a.x - 0.3, ayMid);
            ctx.lineTo(a.x - 0.3, a.yCrest);
            ctx.lineTo(b.x + 0.3, b.yCrest);
            ctx.lineTo(b.x + 0.3, byMid);
            ctx.closePath();
            ctx.fill();
          }

          if (a.snowFrac > 0.45 || a.crestKm > snowlineKm || liveSnowCover > 0.28) {
            const snowDepth =
              a.snowFrac > 0.45
                ? 0.65
                : Math.max(liveSnowCover * 0.55, Math.min(0.42, (a.crestKm - snowlineKm) * 0.24));
            const aySnow = a.yCrest + (a.yBase - a.yCrest) * snowDepth;
            const bySnow = b.yCrest + (b.yBase - b.yCrest) * snowDepth;
            ctx.fillStyle = shadeTerrainRgb([238, 244, 252], a.worldAz, 0.24, 0.08);
            ctx.beginPath();
            ctx.moveTo(a.x - 0.3, aySnow);
            ctx.lineTo(a.x - 0.3, a.yCrest);
            ctx.lineTo(b.x + 0.3, b.yCrest);
            ctx.lineTo(b.x + 0.3, bySnow);
            ctx.closePath();
            ctx.fill();
          }

          // Wet intertidal rock/sand band along coastal land margins when tidal range is active & tide is below high water
          if (hasAtm && tidalRangeM > 0.2 && (!obsIsLand || a.coastTaper < 0.85)) {
            const highWaterDropPx = Math.max(0.0, (0.5 * tidalRangeM - tideOffsetM) * 1.15);
            if (highWaterDropPx > 0.4) {
              ctx.fillStyle = "rgba(38, 34, 28, 0.68)";
              ctx.fillRect(
                a.x - 0.3,
                a.yBase - highWaterDropPx,
                Math.max(1.2, b.x - a.x + 0.6),
                highWaterDropPx + 0.6
              );
            }
            ctx.fillStyle = "rgba(224, 242, 254, 0.45)";
            ctx.fillRect(a.x - 0.2, a.yBase - 0.6, Math.max(1.2, b.x - a.x + 0.4), 1.1);
          }

          if (a.riverFrac > 0.42) {
            const ry = a.yBase - (a.yBase - a.yCrest) * 0.16;
            ctx.fillStyle = `rgba(56, 189, 248, ${(0.55 * Math.max(0.3, baseLight)).toFixed(3)})`;
            ctx.fillRect(a.x, ry, Math.max(1.5, b.x - a.x + 0.5), 1.8);
          }
        }

        ctx.strokeStyle = "rgba(15, 23, 42, 0.52)";
        ctx.lineWidth = 0.9;
        ctx.beginPath();
        ctx.moveTo(seg[0].x, seg[0].yCrest);
        for (let j = 1; j < seg.length; j++) {
          ctx.lineTo(seg[j].x, seg[j].yCrest);
        }
        ctx.stroke();
      }
      idx = endIdx + 1;
    }
  }

  // 6d. Immediate Foreground (Observer's own clicked tile `obs_*` at bottom of vantage frame)
  const isCoastalLand =
    hasAtm &&
    obsIsLand &&
    (anyWaterNeighborInFov ||
      (Array.isArray(vantage.neighbors) && vantage.neighbors.some((nb) => !nb.isLand)));

  if (obsIsLand) {
    const obsRockRgb = vantageRockBaseRgb(
      vantage.obsRockName,
      vantage.obsRockKind,
      vantage.obsFelsity
    );
    const obsSurfRgb = blendTileSurfaceRgb(
      obsRockRgb,
      vantage.obsVegFrac || 0.0,
      vantage.obsMountainosityNorm || 0.0,
      vantage.obsKoeppen || 0,
      vantage.obsElevKm || 0.0
    );
    const fgTopCss = shadeTerrainRgb(obsSurfRgb, viewAzDeg, 0.22, 0.03);
    const fgBotRgb = [
      obsSurfRgb[0] * 0.76 + obsRockRgb[0] * 0.24,
      obsSurfRgb[1] * 0.76 + obsRockRgb[1] * 0.24,
      obsSurfRgb[2] * 0.76 + obsRockRgb[2] * 0.24,
    ];
    const fgBotCss = shadeTerrainRgb(fgBotRgb, viewAzDeg + 20.0, 0.18, 0.0);
    const fgBaseAlt = isCoastalLand ? -7.2 - Math.max(-2.0, tideVisualDeg * 0.65) : -6.0;
    const fgGrad = ctx.createLinearGradient(0, altToY(fgBaseAlt + 0.2), 0, cssH);
    fgGrad.addColorStop(0.0, fgTopCss);
    fgGrad.addColorStop(1.0, fgBotCss);

    const fgSteps = 32;
    const obsMount = vantage.obsMountainosityNorm || 0.0;

    // If coastal land & low tide, draw exposed dark wet intertidal sand/mudflat just above foreground shore
    if (isCoastalLand && tidalRangeM > 0.15) {
      const exposedDeg = Math.max(0.0, (0.5 * tidalRangeM - tideOffsetM) * 0.75);
      if (exposedDeg > 0.15) {
        const yWetTop = altToY(fgBaseAlt + exposedDeg);
        const yWetBot = altToY(fgBaseAlt - 1.2);
        ctx.fillStyle = shadeTerrainRgb([64, 56, 44], viewAzDeg, 0.12, 0.04);
        ctx.fillRect(0, yWetTop, cssW, Math.max(2.0, yWetBot - yWetTop));
        ctx.strokeStyle = "rgba(224, 242, 254, 0.52)";
        ctx.lineWidth = 1.0;
        ctx.beginPath();
        ctx.moveTo(0, yWetTop);
        ctx.lineTo(cssW, yWetTop);
        ctx.stroke();
      }
    }

    ctx.fillStyle = fgGrad;
    ctx.beginPath();
    ctx.moveTo(0, cssH);
    for (let k = 0; k <= fgSteps; k++) {
      const dAz = -50.0 + (100.0 * k) / fgSteps;
      const wAz = ((viewAzDeg + dAz) % 360.0 + 360.0) % 360.0;
      const undul =
        vantageRidgeNoise(wAz * 1.5, (view.selectedTileIdx || 0) + 101) *
        (0.55 + 2.4 * obsMount);
      const fgAlt = fgBaseAlt + undul;
      ctx.lineTo(dAzToX(dAz), altToY(fgAlt));
    }
    ctx.lineTo(cssW, cssH);
    ctx.closePath();
    ctx.fill();

    // Coastal storm surge / high-tide inundation lapping onto low-lying coastal foreground
    if (isCoastalLand && (vantage.obsElevKm || 0.0) * 1000.0 < Math.max(0.0, tideOffsetM)) {
      const floodAlt = Math.min(-3.5, fgBaseAlt + Math.min(4.0, tideOffsetM * 0.65));
      const yFlood = altToY(floodAlt);
      ctx.fillStyle = "rgba(14, 116, 144, 0.56)";
      ctx.fillRect(0, yFlood, cssW, cssH - yFlood);
      ctx.strokeStyle = "rgba(186, 230, 253, 0.65)";
      ctx.lineWidth = 1.1;
      ctx.beginPath();
      ctx.moveTo(0, yFlood);
      ctx.lineTo(cssW, yFlood);
      ctx.stroke();
    }

    ctx.strokeStyle = "rgba(15, 23, 42, 0.38)";
    ctx.lineWidth = 0.85;
    ctx.beginPath();
    for (let k = 0; k <= fgSteps; k++) {
      const dAz = -50.0 + (100.0 * k) / fgSteps;
      const wAz = ((viewAzDeg + dAz) % 360.0 + 360.0) % 360.0;
      const undul =
        vantageRidgeNoise(wAz * 1.5, (view.selectedTileIdx || 0) + 101) *
        (0.55 + 2.4 * obsMount);
      const px = dAzToX(dAz);
      const py = altToY(fgBaseAlt + undul);
      if (k === 0) ctx.moveTo(px, py);
      else ctx.lineTo(px, py);
    }
    ctx.stroke();

    if (vantage.obsIsRiver) {
      const rivAzOff =
        Math.sin(((viewAzDeg * Math.PI) / 180.0) * 2.0 + (view.selectedTileIdx || 0) * 0.3) * 18.0;
      const rxTop = dAzToX(rivAzOff * 0.45);
      const rxBot = dAzToX(rivAzOff);
      const ryTop = altToY(fgBaseAlt);
      ctx.fillStyle = `rgba(56, 189, 248, ${(0.68 * Math.max(0.28, baseLight)).toFixed(3)})`;
      ctx.beginPath();
      ctx.moveTo(rxTop - 2.0, ryTop);
      ctx.lineTo(rxTop + 2.0, ryTop);
      ctx.lineTo(rxBot + 9.0, cssH);
      ctx.lineTo(rxBot - 9.0, cssH);
      ctx.closePath();
      ctx.fill();
    }
  }

  // 6e. Foreground Falling Rain Streaks or Drifting Snowflakes
  if (hasAtm && (rainMmHr > 0.08 || snowMmHr > 0.04)) {
    const ph = view.tileWeatherAnimPhase || 0.0;
    const crossWind = Math.sin((wrapDeltaAzDeg(windAzDeg, viewAzDeg) * Math.PI) / 180.0);
    const slantPx = crossWind * Math.min(14.0, windSpeedMps * 0.65);

    if (rainMmHr > 0.08) {
      const nDrops = Math.min(72, Math.round(14 + rainMmHr * 6.5));
      ctx.strokeStyle = `rgba(186, 230, 253, ${Math.min(0.68, 0.26 + rainMmHr * 0.04).toFixed(3)})`;
      ctx.lineWidth = rainMmHr > 4.0 ? 1.25 : 0.95;
      ctx.beginPath();
      for (let i = 0; i < nDrops; i++) {
        const fx = (((i * 0.618034 + ph * 0.35 * crossWind) % 1.0) + 1.0) % 1.0;
        const fy = (((i * 0.381966 + ph * 1.65) % 1.0) + 1.0) % 1.0;
        const x0 = fx * cssW;
        const y0 = fy * cssH;
        const dropLen = 9.0 + Math.min(10.0, rainMmHr * 0.9);
        ctx.moveTo(x0, y0);
        ctx.lineTo(x0 + slantPx, y0 + dropLen);
      }
      ctx.stroke();
    } else if (snowMmHr > 0.04) {
      const nFlakes = Math.min(64, Math.round(12 + snowMmHr * 10.0));
      ctx.fillStyle = "rgba(248, 250, 252, 0.84)";
      for (let i = 0; i < nFlakes; i++) {
        const flutter = Math.sin(ph * 2.4 + i * 1.7) * 0.025;
        const fx = (((i * 0.754877 + ph * 0.14 * crossWind + flutter) % 1.0) + 1.0) % 1.0;
        const fy = (((i * 0.569840 + ph * 0.42) % 1.0) + 1.0) % 1.0;
        const rF = 1.1 + (i % 3) * 0.45;
        ctx.beginPath();
        ctx.arc(fx * cssW, fy * cssH, rF, 0, Math.PI * 2);
        ctx.fill();
      }
    }
  }

  // 7. Subtle HUD overlays: +90° zenith / altitude ticks, compass bearings & live tide/weather badge
  ctx.strokeStyle = "rgba(148, 163, 184, 0.20)";
  ctx.lineWidth = 0.7;
  ctx.setLineDash([2, 3]);
  ctx.beginPath();
  ctx.moveTo(0, yHor0);
  ctx.lineTo(cssW, yHor0);
  ctx.stroke();
  ctx.setLineDash([]);

  ctx.fillStyle = "rgba(203, 213, 225, 0.62)";
  ctx.font = "8px sans-serif";
  ctx.textAlign = "left";
  ctx.textBaseline = "top";
  ctx.fillText("+90°", 3, 2);
  ctx.textBaseline = "middle";
  for (const deg of [60, 30, 0]) {
    const ty = altToY(deg);
    ctx.strokeStyle = "rgba(148, 163, 184, 0.35)";
    ctx.beginPath();
    ctx.moveTo(0, ty);
    ctx.lineTo(4, ty);
    ctx.stroke();
    ctx.fillText(`${deg}°`, 5, ty);
  }

  const cardinals = [
    [0, "N"],
    [45, "NE"],
    [90, "E"],
    [135, "SE"],
    [180, "S"],
    [225, "SW"],
    [270, "W"],
    [315, "NW"],
  ];
  ctx.textAlign = "center";
  ctx.textBaseline = "top";
  ctx.font = "bold 8.5px sans-serif";
  for (const [cAz, cLbl] of cardinals) {
    const dAz = wrapDeltaAzDeg(cAz, viewAzDeg);
    if (Math.abs(dAz) <= 46.0) {
      const cxTick = dAzToX(dAz);
      ctx.strokeStyle = "rgba(226, 232, 240, 0.55)";
      ctx.beginPath();
      ctx.moveTo(cxTick, 0);
      ctx.lineTo(cxTick, 4);
      ctx.stroke();
      ctx.fillStyle = cLbl.length === 1 ? "#fde047" : "rgba(226, 232, 240, 0.80)";
      ctx.fillText(cLbl, cxTick, 5);
    }
  }

  // Live Tide / Weather compact badge in bottom-right corner when `liveEnv` is present
  if (live && (!obsIsLand || isCoastalLand || rainMmHr > 0.08 || snowMmHr > 0.04)) {
    const parts = [];
    if (!obsIsLand || isCoastalLand) {
      const signT = tideOffsetM >= 0 ? "+" : "";
      parts.push(`Tide ${signT}${tideOffsetM.toFixed(2)}m`);
    }
    if (rainMmHr > 0.08) {
      parts.push(`Rain ${rainMmHr.toFixed(1)}mm/h`);
    } else if (snowMmHr > 0.04) {
      parts.push(`Snow ${snowMmHr.toFixed(1)}mm/h`);
    }
    if (parts.length > 0) {
      const txt = parts.join(" · ");
      ctx.font = "bold 8px sans-serif";
      const tw = ctx.measureText(txt).width + 8;
      ctx.fillStyle = "rgba(15, 23, 42, 0.72)";
      ctx.fillRect(cssW - tw - 4, cssH - 15, tw, 12);
      ctx.fillStyle = "#7dd3fc";
      ctx.textAlign = "right";
      ctx.textBaseline = "middle";
      ctx.fillText(txt, cssW - 8, cssH - 9);
    }
  }

  ctx.strokeStyle = "rgba(56, 189, 248, 0.65)";
  ctx.lineWidth = 1.1;
  ctx.beginPath();
  ctx.moveTo(cssW * 0.5, cssH - 5);
  ctx.lineTo(cssW * 0.5, cssH);
  ctx.stroke();

  ctx.restore();
}

export function ensureTileSkyLoop(view) {
  if (!view || typeof window === "undefined") return;
  if (view.tileSkyRafId) return;
  let lastTs = performance.now();
  const stepSky = (now) => {
    if (hostGetActiveView() !== view || !view.tileSky) {
      view.tileSkyRafId = null;
      return;
    }
    const dt = Math.min(0.1, Math.max(0.0, (now - lastTs) * 0.001));
    lastTs = now;
    const live = view.tileLiveEnv || null;
    if (live && live.simLocked) {
      const needsWeatherAnim =
        (live.rainMmHr || 0) > 0.06 ||
        (live.snowMmHr || 0) > 0.03 ||
        (live.windSpeedMps || 0) > 4.5 ||
        (live.cloudLow || 0) > 0.08;
      if (needsWeatherAnim) {
        view.tileWeatherAnimPhase = ((view.tileWeatherAnimPhase || 0.0) + dt * 0.85) % 1000.0;
        paintTileSkyCanvas(view);
        paintTileVantageCanvas(view);
        view.tileSkyRafId = window.requestAnimationFrame(stepSky);
      } else {
        view.tileSkyRafId = null;
      }
      return;
    }

    if (view.tileSkyAnimated !== false) {
      const daysPerYear = Math.max(1, view.tileSky.daysPerYear || 365);
      view.tileSkyHourFrac = (view.tileSkyHourFrac || 0.0) + dt * 0.22;
      if (view.tileSkyHourFrac >= 1.0) {
        const addDays = Math.floor(view.tileSkyHourFrac);
        view.tileSkyHourFrac -= addDays;
        let nextDay = (view.tileSkyDay || 0) + addDays;
        if (nextDay >= daysPerYear) {
          const addYears = Math.floor(nextDay / daysPerYear);
          nextDay = nextDay % daysPerYear;
          view.tileSkyYear = Math.max(0, (view.tileSkyYear || 0) + addYears);
          const yearInput = document.getElementById("tile-sky-year-input");
          if (yearInput && document.activeElement !== yearInput) {
            yearInput.value = String(view.tileSkyYear);
          }
        }
        view.tileSkyDay = nextDay;
        const daySlider = document.getElementById("tile-sky-day-slider");
        if (daySlider) {
          daySlider.value = String(view.tileSkyDay);
        }
        const dayLabel = document.getElementById("tile-sky-day-label");
        if (dayLabel) {
          dayLabel.textContent = `Day in Year: Day ${view.tileSkyDay} / ${Math.max(1, daysPerYear - 1)}`;
        }
      }
    }
    paintTileSkyCanvas(view);
    paintTileVantageCanvas(view);
    view.tileSkyRafId = window.requestAnimationFrame(stepSky);
  };
  view.tileSkyRafId = window.requestAnimationFrame(stepSky);
}

export function clearTileSkyAndVantageOnView(view) {
  if (!view) return;
  view.tileSky = null;
  view.tileVantage = null;
  view.tileLiveEnv = null;
}

export function scheduleTileSkyAndVantagePaint(view) {
  if (!view || !view.tileSky || typeof window === "undefined") return;
  window.requestAnimationFrame(() => {
    paintTileSkyCanvas(view);
    paintTileVantageCanvas(view);
    ensureTileSkyLoop(view);
  });
  setTimeout(() => {
    paintTileSkyCanvas(view);
    paintTileVantageCanvas(view);
    ensureTileSkyLoop(view);
  }, 45);
}

export function applyParsedTileToView(view, parsedTile) {
  if (!view || !parsedTile || !parsedTile.hasTile) {
    clearTileSkyAndVantageOnView(view);
    return null;
  }
  const sky = parsedTile.sky || null;
  const vantage = parsedTile.vantage || null;
  const liveEnv = parsedTile.liveEnv || null;
  view.tileSky = sky;
  view.tileVantage = vantage;
  view.tileLiveEnv = liveEnv;
  view.tileStratigraphy = parsedTile.stratigraphy || null;

  if (view.tileSkyYear === undefined) view.tileSkyYear = 0;
  if (view.tileSkyDay === undefined) view.tileSkyDay = 0;
  if (view.tileSkyHourFrac === undefined) view.tileSkyHourFrac = 0.35;
  if (view.tileSkyAnimated === undefined) view.tileSkyAnimated = true;

  const daysPerYear = sky && sky.daysPerYear ? sky.daysPerYear : 365;
  if (liveEnv && liveEnv.simLocked) {
    view.tileSkyYear = Math.max(0, liveEnv.simYear | 0);
    view.tileSkyDay = Math.floor(Math.max(0.0, liveEnv.simDay || 0.0)) % Math.max(1, daysPerYear);
    view.tileSkyHourFrac = Math.max(0.0, Math.min(0.9999, liveEnv.simHourFrac || 0.0));
  } else if (view.tileSkyDay >= daysPerYear) {
    view.tileSkyDay = view.tileSkyDay % Math.max(1, daysPerYear);
  }

  const nbs = vantage && Array.isArray(vantage.neighbors) ? vantage.neighbors : [];
  if (view.tileVantageTileIdx !== view.selectedTileIdx) {
    view.tileVantageTileIdx = view.selectedTileIdx;
    if (nbs.length > 0 && typeof view.tileVantagePreferredAzDeg === "number") {
      let bestIdx = 0;
      let bestDiff = 999.0;
      for (let i = 0; i < nbs.length; i++) {
        const diff = Math.abs(wrapDeltaAzDeg(nbs[i].azDeg || 0.0, view.tileVantagePreferredAzDeg));
        if (diff < bestDiff) {
          bestIdx = diff < bestDiff ? i : bestIdx;
          bestDiff = diff;
          bestIdx = i;
        }
      }
      view.tileVantageNeighborIdx = bestIdx;
    } else {
      view.tileVantageNeighborIdx = 0;
    }
  } else if (nbs.length > 0) {
    view.tileVantageNeighborIdx =
      ((view.tileVantageNeighborIdx | 0) % nbs.length + nbs.length) % nbs.length;
  } else {
    view.tileVantageNeighborIdx = 0;
  }

  const latVal = sky && typeof sky.latDeg === "number" ? sky.latDeg : 0.0;
  const lonVal = sky && typeof sky.lonDeg === "number" ? sky.lonDeg : 0.0;
  const latStr = `${Math.abs(latVal).toFixed(1)}°${latVal >= 0 ? "N" : "S"}`;
  const lonStr = `${Math.abs(lonVal).toFixed(1)}°${lonVal >= 0 ? "E" : "W"}`;
  const vSum = vantageSummaryStrings(view);

  if (parsedTile.stateId !== undefined && parsedTile.stateId >= 0) {
    view.selectedStateId = parsedTile.stateId;
  }

  return {
    hasTile: true,
    displayIdx: parsedTile.displayIdx ?? view.selectedTileIdx,
    stateId: parsedTile.stateId ?? -1,
    title: parsedTile.title || "",
    locationSummary: parsedTile.locationSummary || "",
    populationSummary: parsedTile.populationSummary || "",
    technology: parsedTile.technology || "",
    nationalities: parsedTile.nationalities || [],
    languages: parsedTile.languages || [],
    flora: parsedTile.flora || [],
    fauna: parsedTile.fauna || [],
    skyYear: view.tileSkyYear,
    skyDay: view.tileSkyDay,
    skyDaysPerYear: daysPerYear,
    skyLatLonSummary: `${latStr}, ${lonStr}`,
    skyAnimated: view.tileSkyAnimated !== false,
    vantageBearingSummary: vSum.bearingSummary,
    vantageNeighborSummary: vSum.neighborSummary,
  };
}

/**
 * Crisp hi-DPI 48-hour Canvas 2D sparkline / meteogram renderer.
 * `seriesList`: array of `{ values: number[], color: string, fillColor?: string, kind?: "line" | "bars", min?: number, max?: number, zeroLine?: boolean, label?: string }`
 */
export function paintSparklineCanvas(canvasId, seriesList, options = {}) {
  if (typeof document === "undefined") return;
  const canvas = document.getElementById(canvasId);
  if (!canvas) return;

  const rect = canvas.getBoundingClientRect();
  const cssW = Math.max(180, Math.round(rect.width || 280));
  const cssH = Math.max(70, Math.round(rect.height || 94));
  const dpr = window.devicePixelRatio || 1;
  const targetW = Math.round(cssW * dpr);
  const targetH = Math.round(cssH * dpr);
  if (canvas.width !== targetW || canvas.height !== targetH) {
    canvas.width = targetW;
    canvas.height = targetH;
  }

  const ctx = canvas.getContext("2d");
  if (!ctx) return;
  ctx.save();
  ctx.scale(dpr, dpr);

  ctx.fillStyle = "#050b16";
  ctx.fillRect(0, 0, cssW, cssH);

  const padL = 8;
  const padR = 8;
  const padT = 10;
  const padB = 14;
  const plotW = Math.max(20, cssW - padL - padR);
  const plotH = Math.max(20, cssH - padT - padB);

  // Subtle 12-hour vertical gridlines across 48h window
  ctx.strokeStyle = "rgba(51, 65, 85, 0.45)";
  ctx.lineWidth = 0.75;
  for (let q = 0; q <= 4; q++) {
    const gx = padL + (plotW * q) / 4.0;
    ctx.beginPath();
    ctx.moveTo(gx, padT);
    ctx.lineTo(gx, padT + plotH);
    ctx.stroke();
  }

  const validSeries = Array.isArray(seriesList)
    ? seriesList.filter((s) => s && Array.isArray(s.values) && s.values.length > 0)
    : [];

  if (validSeries.length === 0) {
    ctx.fillStyle = "#64748b";
    ctx.font = "10px sans-serif";
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    ctx.fillText(options.emptyText || "Step simulation (+1h / Play) to record 48h trace", cssW * 0.5, cssH * 0.48);
    ctx.restore();
    return;
  }

  for (const s of validSeries) {
    const vals = s.values;
    const n = vals.length;
    let lo = typeof s.min === "number" ? s.min : Infinity;
    let hi = typeof s.max === "number" ? s.max : -Infinity;
    for (const v of vals) {
      if (v < lo) lo = v;
      if (v > hi) hi = v;
    }
    if (s.zeroLine) {
      lo = Math.min(lo, -0.05);
      hi = Math.max(hi, 0.05);
    }
    if (!Number.isFinite(lo) || !Number.isFinite(hi) || hi - lo < 1e-4) {
      lo = (Number.isFinite(lo) ? lo : 0.0) - 0.5;
      hi = lo + 1.0;
    } else {
      const margin = (hi - lo) * 0.08;
      if (typeof s.min !== "number") lo -= margin;
      if (typeof s.max !== "number") hi += margin;
    }

    const xOf = (idx) =>
      n <= 1 ? padL + plotW : padL + (idx / Math.max(1, (options.maxPoints || Math.max(n, 2)) - 1)) * plotW;
    const xAligned = (idx) => {
      const cap = Math.max(n, options.windowHours || n);
      const startOff = cap - n;
      return cap <= 1 ? padL + plotW : padL + ((startOff + idx) / (cap - 1)) * plotW;
    };
    void xOf;
    const yOf = (v) => padT + (1.0 - (v - lo) / (hi - lo)) * plotH;

    if (s.zeroLine && lo <= 0.0 && hi >= 0.0) {
      const y0 = yOf(0.0);
      ctx.save();
      ctx.strokeStyle = "rgba(148, 163, 184, 0.48)";
      ctx.lineWidth = 0.85;
      ctx.setLineDash([3, 3]);
      ctx.beginPath();
      ctx.moveTo(padL, y0);
      ctx.lineTo(padL + plotW, y0);
      ctx.stroke();
      ctx.restore();
    }

    if (s.kind === "bars") {
      const cap = Math.max(n, options.windowHours || 48);
      const barW = Math.max(1.8, (plotW / cap) * 0.72);
      ctx.fillStyle = s.color || "rgba(56, 189, 248, 0.65)";
      const yBase = padT + plotH;
      for (let i = 0; i < n; i++) {
        const v = Math.max(0.0, vals[i]);
        if (v <= 1e-4) continue;
        const bx = xAligned(i) - barW * 0.5;
        const by = yOf(v);
        ctx.fillRect(bx, by, barW, Math.max(1.0, yBase - by));
      }
      continue;
    }

    if (s.fillColor && n >= 2) {
      const yBase = s.zeroLine && lo <= 0.0 && hi >= 0.0 ? yOf(0.0) : padT + plotH;
      ctx.fillStyle = s.fillColor;
      ctx.beginPath();
      ctx.moveTo(xAligned(0), yBase);
      for (let i = 0; i < n; i++) {
        ctx.lineTo(xAligned(i), yOf(vals[i]));
      }
      ctx.lineTo(xAligned(n - 1), yBase);
      ctx.closePath();
      ctx.fill();
    }

    ctx.strokeStyle = s.color || "#38bdf8";
    ctx.lineWidth = s.lineWidth || 1.65;
    ctx.beginPath();
    for (let i = 0; i < n; i++) {
      const px = xAligned(i);
      const py = yOf(vals[i]);
      if (i === 0) ctx.moveTo(px, py);
      else ctx.lineTo(px, py);
    }
    ctx.stroke();

    // Latest point dot
    const lastX = xAligned(n - 1);
    const lastY = yOf(vals[n - 1]);
    ctx.fillStyle = s.color || "#38bdf8";
    ctx.beginPath();
    ctx.arc(lastX, lastY, 2.5, 0, Math.PI * 2);
    ctx.fill();
  }

  // Time axis labels (-48h .. Now)
  ctx.fillStyle = "#64748b";
  ctx.font = "8.5px sans-serif";
  ctx.textBaseline = "bottom";
  ctx.textAlign = "left";
  ctx.fillText("-48h", padL, cssH - 1);
  ctx.textAlign = "center";
  ctx.fillText("-24h", padL + plotW * 0.5, cssH - 1);
  ctx.textAlign = "right";
  ctx.fillText("Now", padL + plotW, cssH - 1);

  if (Array.isArray(options.legend) && options.legend.length > 0) {
    let lx = padL + 2;
    ctx.font = "bold 8px sans-serif";
    ctx.textAlign = "left";
    ctx.textBaseline = "top";
    for (const item of options.legend) {
      ctx.fillStyle = item.color || "#cbd5e1";
      ctx.fillText(item.text, lx, 2);
      lx += ctx.measureText(item.text).width + 9;
    }
  }

  ctx.restore();
}

// PureScript FFI exports for `Tec11.SkyVantage`
export const setTileSkyYear = (year) => () => {
  const activeView = hostGetActiveView();
  if (!activeView || !activeView.session) return;
  if (activeView.tileLiveEnv && activeView.tileLiveEnv.simLocked) return;
  activeView.tileSkyYear = Math.max(0, year | 0);
  hostRefreshView(activeView);
};

export const setTileSkyDay = (day) => () => {
  const activeView = hostGetActiveView();
  if (!activeView || !activeView.session) return;
  if (activeView.tileLiveEnv && activeView.tileLiveEnv.simLocked) return;
  const maxDay =
    activeView.tileSky && activeView.tileSky.daysPerYear
      ? Math.max(0, activeView.tileSky.daysPerYear - 1)
      : 364;
  activeView.tileSkyDay = Math.max(0, Math.min(maxDay, day | 0));
  hostRefreshView(activeView);
};

export const toggleTileSkyAnimation = () => {
  const activeView = hostGetActiveView();
  if (!activeView || !activeView.session) return;
  if (activeView.tileLiveEnv && activeView.tileLiveEnv.simLocked) return;
  activeView.tileSkyAnimated = activeView.tileSkyAnimated === false;
  hostRefreshView(activeView);
};

export const stepTileVantageDirection = (delta) => () => {
  const activeView = hostGetActiveView();
  if (!activeView || !activeView.tileVantage || !Array.isArray(activeView.tileVantage.neighbors)) {
    return;
  }
  const nbs = activeView.tileVantage.neighbors;
  if (nbs.length === 0) return;
  const curIdx = activeView.tileVantageNeighborIdx | 0;
  const nextIdx = ((curIdx + (delta | 0)) % nbs.length + nbs.length) % nbs.length;
  activeView.tileVantageNeighborIdx = nextIdx;
  activeView.tileVantagePreferredAzDeg = nbs[nextIdx].azDeg || 0.0;

  const vSum = vantageSummaryStrings(activeView);
  const dirEl = document.getElementById("tile-vantage-dir-label");
  if (dirEl) {
    dirEl.textContent = vSum.bearingSummary;
  }
  const sumEl = document.getElementById("tile-vantage-summary-label");
  if (sumEl) {
    sumEl.textContent = vSum.neighborSummary;
  }
  paintTileSkyCanvas(activeView);
  paintTileVantageCanvas(activeView);
};
