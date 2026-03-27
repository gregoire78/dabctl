# ETI: explication rapide

## C'est quoi ETI

ETI signifie Ensemble Transport Interface.
C'est un format de transport utilisé dans l'écosystème DAB pour véhiculer un multiplex (ensemble de services radio + données) entre des blocs de traitement.

En pratique, ETI encapsule:
- des informations de synchronisation
- des informations de signalisation
- les données audio/données des services du multiplex

## ETI et le flux IQ

Un flux IQ (comme celui d'un RTL-SDR) représente des échantillons radio bruts.
Un flux ETI est un flux déjà démodulé et structuré au niveau DAB.

Chaîne simplifiée:
1. RF + IQ brut (RTL-SDR)
2. Synchronisation OFDM DAB
3. Démodulation/correction/canal
4. Reconstruction des trames DAB
5. Sortie ETI

Donc IQ et ETI ne sont pas le même format:
- IQ = signal brut
- ETI = transport DAB reconstruit

## Pourquoi ETI est utile

ETI est utile pour:
- transporter un multiplex vers un autre logiciel
- archiver un multiplex de manière structurée
- alimenter des outils de décodage/monitoring DAB

## Dans ce projet, où on en est

Actuellement, l'outil lit RTL-SDR et écrit un flux IQ brut sur stdout.
Ce n'est pas encore un ETI reconstruit complet.

Conséquence:
- le pipeline de sortie est prêt
- la partie DSP DAB complète (pour générer un vrai ETI) reste à implémenter

## Référence utile

Pour la logique ETI côté démodulation DAB, voir aussi le projet source d'inspiration:
- https://github.com/JvanKatwijk/eti-stuff
