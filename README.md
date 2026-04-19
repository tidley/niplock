# niplock

A Nostr-synced password vault web app.

## Deployment

Live URL: <https://npub1h9f5qrn2h3zyhqzgxflrvnnmec65ssyaugcxheuvud0prg5pj3asy2692e.nsite.lol/>

## GitHub Pages

The repo includes `.github/workflows/deploy-github-pages.yml` and deploys on every push to `master`.

1. Add a GitHub remote:
   ```bash
   git remote add github https://github.com/<your-user>/<your-repo>.git
   ```
2. Push:
   ```bash
   git push -u github master
   ```
3. In GitHub, open `Settings -> Pages` and set `Source` to `GitHub Actions`.
4. After the workflow completes, your site URL will be:
   `https://<your-user>.github.io/<your-repo>/`
