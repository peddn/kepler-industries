# Architektur und Regeln

Dieses Dokument hält fest, *warum* der Code so aufgebaut ist. Die meisten Regeln
stammen aus einer Analyse der technischen Probleme von Kerbal Space Program 1
und 2. Wer etwas ändert, das einer Regel widerspricht, ändert bitte zuerst
dieses Dokument (mit Begründung).

## Aufbau

| Crate | Aufgabe | Abhängigkeiten |
|---|---|---|
| `crates/sim` | Die gesamte Simulation: Himmelskörper, Bahnen, Atmosphäre, Schiffe, Zeit. Der Zustand (`World`) ist zugleich der Spielstand. | nur `glam` (f64-Mathe) und `serde` |
| `crates/game` | Darstellung und Eingabe mit Bevy. Liest die Simulation und treibt sie voran. | `sim`, `bevy` |

`sim` kennt Bevy nicht. Dadurch kompiliert es in Sekunden, ist vollständig
ohne Fenster testbar (`cargo test -p sim`), und es gibt keinen Spielzustand,
der nur in Bevy-Komponenten steckt.

Einheiten: SI (m, s, kg), alles in `f64`. Zeit = Sekunden seit Spielbeginn.

## Regeln

### 1. Rechengenauigkeit
*KSP:* Unity rechnet in 32 Bit, KSP bräuchte 64 Bit. Folge: zitternde Schiffe,
der „Deep Space Kraken", aufwendige Gegenmaßnahmen (Floating Origin, Krakensbane).

**Regel:** Die Simulation rechnet nur in `f64`. Bevy bekommt jedes Bild nur
Positionen *relativ zum Kamerafokus* als `f32` (`camera::to_view`). Der Fokus
liegt in Bevy im Ursprung, die Kamera ist nie weiter entfernt als der
Zoomabstand. Es gibt keine `f32`-Weltkoordinaten.

### 2. Ein einziges Bezugssystem
*KSP:* Schaltet unterhalb von ~100 km auf ein mitrotierendes System um.
Folge: Geschwindigkeitsangaben weichen um die Rotationsgeschwindigkeit ab,
Positionen sind inkonsistent, Spieler vermuten Bahndrift in genau diesen Höhen.

**Regel:** Jede Bahn ist im nicht rotierenden System ihres Körpers
beschrieben (Ursprung im Mittelpunkt, +Z = Nordpol). Die Rotation steckt nur
in den Umrechnungen Oberfläche ↔ Raum in `body.rs`.

### 3. Keine Bahndrift
*KSP:* Bahnen werden bei Zeitraffer-, Fokus- und SOI-Wechseln neu berechnet und
gerundet; Satellitennetze laufen auseinander. KSP 2 hatte einen „Orbital
Decay"-Bug, der per Patch nur „stark reduziert" wurde.

**Regel:** Ein Keplerabschnitt speichert seinen Zustand zu Beginn (Epoche) und
verändert ihn nie. Jede Position wird direkt daraus berechnet
(`kepler::propagate`), bei Ellipsen nach Abzug ganzer Umläufe. Neue Bahnen
entstehen nur durch ausdrückliche Ereignisse.
*Test:* `no_drift_after_thousand_orbits`, `stable_orbit_never_changes`.

### 4. Kein „aktives Schiff", keine Physik-Blase
*KSP:* Nur Schiffe im Umkreis von ~2,5 km werden voll simuliert. Andere fliegen
ohne Luftwiderstand oder werden in der Atmosphäre gelöscht. Für ein Spiel mit
automatischen Routen wäre das fatal.

**Regel:** Alle Schiffe werden gleich behandelt, egal ob sie im Bild sind.
Flugabschnitte mit Luft oder Schub werden einmal vorausberechnet und danach
nur abgespielt (`Segment::Integrated`).

### 5. Zeitraffer gilt immer
*KSP:* Kein Schub unter Zeitraffer, Ionentriebwerke brennen stundenlang in Echtzeit.

**Regel:** Weil berechnete Abschnitte nur abgespielt werden, funktioniert der
Zeitraffer in jeder Lage, auch in der Luft und beim Brennen.

### 6. Ergebnis unabhängig von der Schrittgröße
*KSP:* SOI-Wechsel im Hintergrund weichen bei hohem Zeitraffer von der Vorhersage ab.

**Regel:** Ereignisse (Eintritt in die Luft, Austritt, Bodenkontakt, Beginn und
Ende eines Brennmanövers) werden exakt bestimmt (Keplerbahn: analytisch,
berechneter Abschnitt: im Schritt interpoliert bzw. genau bis zum Zeitpunkt
gerechnet). Ein Abschnitt hängt nur von seinem Anfangszustand und dem
geplanten Manöver ab. Der Integrator hat eine feste Schrittweite, unabhängig
von Bildrate und Zeitraffer.
*Test:* `result_is_independent_of_step_size`, `node_result_is_independent_of_step_size`
(1 Schritt == 10 000 Schritte, bitgenau).

### 7. Keine Bauteil-Physik
*KSP:* Wackelnde Raketen, Lag ab einigen hundert Teilen, Phantomkräfte. In KSP 2
erzeugte ein Fix gegen das Wackeln einen wochenlang unbemerkten Treibstoff-Bug.

**Regel:** Ein Schiff ist ein Massepunkt mit Werten. Keine Physik-Engine.

### 8. Am Boden ist ein Zustand
*KSP:* Gelandete Schiffe hüpfen oder explodieren beim Laden.

**Regel:** „Am Boden" ist ein Abschnitt mit Körper, Breite und Länge
(`Segment::Surface`). Er dreht sich mit dem Körper und kann nicht hüpfen.

### 9. Viele Schiffe sind billig
*KSP:* Auch Schiffe „auf Schienen" und Trümmer kosten Rechenzeit.

**Regel:** Jedes Schiff kennt das Ende seines aktuellen Abschnitts. Pro Bild ist
das ein Zahlenvergleich. Gerechnet wird nur, wenn ein Abschnitt endet. Wracks
am Boden kosten nichts. Eine globale Ereignis-Warteschlange kommt erst, wenn
Schiffe miteinander wechselwirken (Rendezvous).

### 10. Kleine, spielbare Schritte
*KSP 2:* Studio 2024 geschlossen, versprochene Inhalte nie geliefert.

**Regel:** Jeder PR ist klein und spielbar. Die Simulation wird durch Tests
abgesichert, die ohne Bevy laufen.

### 11. Spielstände bitgenau
Der Test `save_and_load_round_trip` hat gezeigt: `serde_json` liest Zahlen
ohne die Option `float_roundtrip` nicht bitgenau zurück. Jedes künftige
Speicherformat muss Zahlen exakt erhalten. Der Spielstand trägt eine
Versionsnummer (`SAVE_VERSION`).

### 12. Die Vorschau ist die Ausführung
Ein Manöver-Node ist der Plan selbst: `set_node` berechnet die Zeitleiste neu,
und diese Zeitleiste wird später genau so abgespielt. Es gibt kein eigenes
Vorschausystem, das von der Ausführung abweichen könnte.
*Test:* `impact_prediction_matches_wreck`, `clearing_node_restores_trajectory`.

### 13. Bedienelemente auf eigener Ebene
Node und Griffe zeichnet eine zweite Kamera mit eigenem Tiefenpuffer
(`OVERLAY_LAYER`). Sie liegen damit immer über der Welt und verschwinden nicht
im Planeten. Der Versuch mit `depth_bias = -1` zeichnete Linien nur
bruchstückhaft.

## Physikmodell

**Ein Kraftmodell überall:** Schwerkraft + Luftwiderstand + Schub.
Ein Körper ohne Luft ist der Spezialfall Bodendichte 0, Freiflug der
Spezialfall „kein Schub".

**Atmosphäre:** verschobene Exponentialfunktion

    ρ(h) = ρ₀ · (e^(−h/H) − e^(−h_max/H)) / (1 − e^(−h_max/H))   für h < h_max, sonst 0

Am Boden ρ₀, bei `h_max` exakt 0, dazwischen stetig. Oberhalb von `h_max` ist
die Keplerbahn deshalb die *exakte* Lösung desselben Modells, keine Näherung.
Eine reine Exponentialfunktion würde nie null; dann müsste jede Bahn ewig
schrittweise gerechnet werden (Drift, Rechenzeit).

**Luftwiderstand:** `a = −½ · ρ · (c_w·A / m) · |v_rel| · v_rel`, mit `v_rel`
relativ zur mitrotierenden Luft.

**Brennmanöver:** konstanter Schub, konstanter Massenstrom. Die Masse wird
analytisch aus der Zeit berechnet (nicht integriert), dadurch gilt die
Raketengleichung exakt. Die Dauer folgt aus dem Δv:
`m_end = m₀ · e^(−Δv/v_e)`, `Dauer = (m₀ − m_end) / ṁ`. Das Manöver liegt
zeitlich mittig um den Node, die Schubrichtung bleibt im Raum fest (wie in KSP
beim Zielen auf den Node). Reicht der Treibstoff nicht, endet es mit leerem Tank.

**Brennen ganz in der Zukunft:** Läge der Brennbeginn beim Setzen oder Ändern
eines Nodes in der Vergangenheit, rückt der Node genau einen Umlauf weiter
(gleiche Stelle der Bahn, alle Zeiten + Umlaufzeit). Das geht nur auf einer
geschlossenen Bahn, die bis zum Ende des verschobenen Brennens weder Luft noch
Boden berührt; sonst wird der Node abgelehnt (`PlanError::TooLate`).
Dieselbe Regel gilt für einen nicht scharfen Node, dessen Brennbeginn erreicht
wird: Er rückt weiter oder verfällt.

**Zeitleiste eines Schiffs:** Folge von Abschnitten
- `Kepler`: Freiflug ohne Luft, analytisch (universelle Variablen: eine Formel
  für Ellipse, Parabel, Hyperbel).
- `Integrated`: vorausberechnet (RK4, 0,1 s, Stützpunkt jede Sekunde,
  Hermite-Interpolation dazwischen, max. 30 min pro Stück); mit oder ohne Schub.
- `Surface`: am Boden (gelandet oder Wrack).

Vorausberechnet werden bis zu 8 Abschnitte (`LOOKAHEAD`), oder bis die
Zeitleiste offen endet (stabile Bahn, am Boden). Das reicht, um nach einem
Manöver die neue Bahn oder den Aufschlag zu sehen.
Übergänge: Kepler → (Eintritt) → Integrated → (Austritt) → Kepler;
Freiflug → (Brennbeginn) → Integrated mit Schub → (Brennende) → Freiflug;
→ (Bodenkontakt) → Surface/Wrack.

**Aufschlagkreuz:** Es markiert die Stelle *auf dem Boden* und dreht sich mit
dem Planeten. Die Bahnlinie ist im nicht rotierenden System gezeichnet. Bis
zum Aufschlag dreht sich der Planet noch, deshalb liegt das Kreuz vorher nicht
am Ende der Linie; beide treffen sich genau beim Aufschlag. Für den Vergleich
mit Zielgebieten (die sich ebenfalls mitdrehen) ist das die richtige Größe.

## Getroffene Spielentscheidungen

- 3D, kleine Planeten (Kerbin-Größe), **keine Raketenstufen**.
- **Aufschlagkreuz und Bahnlinie:** Das Kreuz zeigt den genauen Punkt auf dem
  Boden (dreht mit dem Planeten), die Bahn wird ohne Planetendrehung
  gezeichnet. Dass das Ende der Linie vor dem Aufschlag nicht am Kreuz liegt,
  ist gewollt (siehe „Aufschlagkreuz“ unter Physikmodell).
- **Ein Manöver-Node pro Schiff.** Der nächste wird geplant, wenn der vorige
  ausgeführt ist.
- **Scharf schalten:** Nur ein scharfer Node wird ausgeführt (`Plan::armed`).
  In der Simulation sind neue Nodes nicht scharf, ein geänderter behält seinen
  Zustand. Ein nicht scharfer Node rückt bei Brennbeginn einen Umlauf weiter
  (siehe „Brennen ganz in der Zukunft“), so geht kein Plan verloren, nur weil
  der Zeitraffer läuft. Die Vorschau ist scharf wie nicht scharf dieselbe. Der
  Zeitraffer-Stopp greift nur bei scharfen Nodes. **Vorerst schaltet das Spiel
  jeden neuen Node sofort scharf**; die Schaltfläche dafür kommt später an
  anderer Stelle in die Oberfläche (siehe UI-Ideen).
- **Brennpunkte:** Gelbe Ringe auf der Bahn markieren Beginn und Ende jedes
  geplanten Brennens, für alle Schiffe mit Node.
- **Griffe:** Ein gezogener Griff folgt der Maus entlang seiner Achse und
  springt beim Loslassen zurück (wie in KSP). Er kann das Δv nicht über den
  Tankinhalt am Brennbeginn hinaus erhöhen. Das begrenzt nur die Bedienung;
  die Simulation erlaubt mehr Δv als im Tank ist (das Brennen endet dann
  leer), damit später vor einem Node nachgetankt werden kann.
- **Während des Brennens** zeigt die Karte die momentane Bahn (die Keplerbahn
  aus dem aktuellen Zustand, wie in KSP). Reine Darstellung, die Simulation
  rechnet unverändert den ganzen Brennvorgang voraus.
- **Zeitraffer-Stopp pro Schiff** (`halt_warp_at_burn`, Taste `H`, anfangs aus):
  Beginnt ein markiertes Schiff zu brennen, hält der Zeitraffer genau am
  Brennbeginn an und schaltet auf 1×. Pro Schiff statt für alle, damit spätere
  automatische Routen den Zeitraffer nicht ständig bremsen. Die Markierung
  gehört zum Spielstand.
- **Startprofil** mit wenigen Parametern (Richtung, Wendebeginn, Ziel-Apoapsis);
  das Schiff kommt mit nicht mehr vollen Tanks oben an. Zirkularisieren per Manöver-Node.
- **Landung über den Manöver-Node:** Die Vorschau rechnet bis zum Boden,
  Aufschlagpunkt als Kreuz, Zielgebiet als Kreisfläche auf der Oberfläche.
  Fallschirme = größere Widerstandsfläche ab einer Auslösehöhe (dasselbe Modell).
  Letzte Bremsung abstrahiert: Restgeschwindigkeit = nötiges Delta-v.
- **Ziel verfehlt:** Mit Treibstoff ein „Hop" (= Startprofil mit niedriger
  Apoapsis, keine eigene Mechanik), ohne Treibstoff oder bei Absturz ein Wrack.
- **Satelliten** sind normale Schiffe. Zu tief ausgesetzt = Fehlschlag (Absturz).
- **Rendezvous:** nächste Annäherung anzeigen, mit Manöver-Nodes verbessern,
  Andocken abstrahiert (kostet Relativgeschwindigkeit als Delta-v),
  Abschnittstyp „angedockt an X".
- **Basis-Schiff:** Rohbau im Orbit, Transporter liefern „Bauteile" per
  Andocken, voll → Vorlage wechselt zum Basis-Schiff. Landet einmalig und
  gründet die Basis dort, wo es aufsetzt.
- **Booster:** abstrahierte Dienstleistung eines Raumhafens (Gebäude einer Basis):
  Kapazität, Rohstoffkosten, Abklingzeit. Das Schiff erscheint mit vollen Tanks
  in einer Parkbahn (echte Keplerbahn aus Breite und Richtung). Boost-back nur
  als Animation.
- **Vessel-Klassen und Gebäude** sind Vorlagen mit Werten; Upgrades ändern Werte.
  Fähigkeiten (Fracht, Antenne, Scanner) sind optionale Felder, keine eigene
  Logik pro Klasse.
- **Ein Auftragssystem** für Geld und Forschungspunkte.

## Merkliste (nicht vergessen)

- Kommunikationsnetz (Verbindung, Reichweite)
- Scans, automatisierte Routen, Ressourcen, Basen, Wirtschaft
- Bergung von Wracks (über Geld/Punkte)
- „Hop zum Ziel": Parameter automatisch suchen, falls Zielen per Hand zu fummelig ist
- **Geparkt:** Bandbreite, Modulslots, Gelände/Höhenkarte, Wiedereintrittshitze
- **Gestrichen:** Technikalterung, Lagrange-Punkte (mit Patched Conics nicht möglich)

## UI-Ideen (nach dem Grundgerüst)

Erst kommt das Grundgerüst, dann die Bedienung. Ideen landen hier, damit
nichts verloren geht.

- **Manöver-Node-Manager:** Übersicht und Bearbeitung der geplanten Nodes
  (sinnvoll, sobald es Manöverketten gibt)
- **Scharf schalten:** Schaltfläche dafür, z. B. im Node-Manager. Die Mechanik
  steckt schon in der Simulation; bis dahin ist jeder Node sofort scharf.

## Reihenfolge

1. Fundament ✓
2. Manöver-Node in 3D ✓ (Schub, endliche Brenndauer, Aufschlagkreuz; dazu
   momentane Bahn beim Brennen, Zeitraffer-Stopp, scharf schalten,
   nächster Umlauf, Griffe mit Tank-Anschlag)
3. Manöverketten: mehrere Nodes pro Schiff, Nodes auf zukünftigen Bahnen
   (Grundlage für Startprofil + Zirkularisieren und für automatische Routen)
4. Startprofil (Zirkularisieren per Node in derselben Kette)
5. Landung (Zielgebiete, Fallschirme, Aufsetzen, Hop, Wrack)
6. Rendezvous und Andocken
7. Mond, Wechsel der Einflusssphäre
8. Fracht und Rohstoffe → Raumhafen und Booster → Rohbau und Basis-Schiff → Basen und Wirtschaft
