# Chapter 20. Updating LCL

**In this chapter you will learn**

* how LCL finds out that a newer release exists, and what **Update available** means;
* how to update LCL Workspace on your PC, and what happens if an update fails;
* how to update LCL for Android, and why Android asks you to confirm;
* what an update never touches: your projects, settings, templates and pairings;
* how LCL knows an update is genuine.

> **Tool note.** Updating needs a release that is signed with the LCL update
> key, and the builds made so far trust no update key yet. Until the first
> signed release is published and installed, **Settings → Updates** says
> *Updates are not set up in this build*, and you install new versions as
> Chapter 2 describes. Everything below is how it works once that release is
> in place.

## 20.1 Where updates come from

LCL looks for new releases in one place only: the published releases of the
official LCL repository on GitHub. Drafts and pre-releases are never offered.
Your PC and your phone look at the same releases, so one published release
reaches both.

A release is offered only when it is **newer** than the one you have, compared
by version number part by part: 0.10.0 is newer than 0.9.0. An older or equal
version is never offered, so an update never moves you backwards.

The version an update talks about is the version of the LCL *product* — the
programs and the app. It is not the version of the LCL language: a new product
release can still teach and check LCL Core 0.3.0.

## 20.2 Checking for updates

LCL checks by itself at most once a day, in the background, when it starts.
You can also check at any time:

* **PC:** open **Settings** (⚙) and find **Updates**. It shows the installed
  version, when LCL last checked, and the current state. Press **Check for
  updates**.
* **Phone:** on the LCL dashboard, press **Updates** (it reads **Update
  available** when one is waiting), then **Check for updates**. The phone does
  not need to be connected to a PC to check.

The state is one of: *Up to date*, *Checking*, *Update available*,
*Downloading*, *Ready to install*, *Installing*, *Failed*, or *Could not check:
offline*. A problem always says what kind it is: no network, an invalid update,
a verification that failed, a download that failed, or an installation that
failed. Being offline never stops you from working in LCL.

When an update is available you see its version, its release date, its size
and its release notes. The notes are shown as plain text.

## 20.3 Updating LCL Workspace on your PC

1. In **Settings → Updates**, press **Update**. LCL downloads the new release
   and checks it (see 20.5); a progress bar shows the download. Only a release
   that passed every check is called *Ready to install*.
2. Press **Install and restart**. If a document has unsaved changes, LCL asks
   you to save or close it first: an update never throws away your work.
3. LCL Workspace closes. The update is installed where LCL is installed now
   (`~/.local/bin` and `~/.local/share/lcl`), with no password and no `sudo`.
   LCL checks the installed programs, then opens LCL Workspace again.

Before anything is replaced, LCL keeps a copy of every file the installation
will write. If installing fails, or the new version does not pass its checks,
that copy is put back and you keep the version you had. The copy is removed as
soon as it is no longer needed.

If the LCL remote service for Android (`lcl-remote`) is installed, it is
updated at the same time. If it was running, it is started again; if it was
stopped, it stays stopped.

## 20.4 Updating LCL for Android

1. Press **Update**. The app downloads the new APK and checks it (see 20.5).
2. The first time, Android asks you to allow LCL to install apps (*Install
   unknown apps*). Press **Allow installing**, switch the permission on, and
   come back: the update continues by itself.
3. Android shows its own confirmation, **Update**. Press it.

Android installs the new version over the old one. Your paired PCs, this
phone's keys and your settings stay exactly as they were, so there is nothing
to pair again. If you cancel, or Android cannot install it, the app you have
is left unchanged.

## 20.5 How LCL knows an update is genuine

Every release carries a small signed statement, its *update manifest*: the
version, the release date, the notes, and for each file its name, its exact
size and its SHA-256 fingerprint. It is signed with the LCL update key, which
is kept offline and never leaves the maintainer's hands; your PC and your
phone carry only the public half.

LCL checks the signature before it believes anything in the manifest, fetches
the files it names from the same release, and installs a file only if its size
and fingerprint are exactly the signed ones. On the phone it also checks that
the APK is LCL (`io.lcl.workspace`), is newer than the installed app, and is
signed with the same key as the app you have. Anything that does not pass is
refused, and nothing is installed.

A signature from a key LCL does not already trust is refused, even if the
manifest names that key. LCL has one update key and keeps it; if it were ever
replaced, the LCL you have would refuse updates signed with the new key, and
you would update it by hand once.

## 20.6 What an update never touches

* your projects and the files in them;
* `~/.config/lcl` — your settings and your Master templates;
* `~/.local/state/lcl` — the remote service's pairings, including this PC's
  identity and the phones it trusts;
* on the phone, the app's data: paired PCs, keys and settings.

Uninstalling is a different operation (Chapter 2); an update never removes
anything of yours.

## Summary

* LCL checks the official releases at most once a day, or when you press
  **Check for updates**, and offers only newer, signed releases.
* On the PC, **Update** then **Install and restart**; a failed installation
  puts the previous version back.
* On the phone, **Update**, allow installing once, and confirm in Android's
  dialog; pairings and settings are kept.
* Maintainers: how a release is signed and published is in
  `packaging/README.md`, section *Publishing an update release*.
