// Song credits: the artists database (artists_info.db3) behind the links on a song's page.
import { getSQL } from '$lib/utils/sqljs';
import { fetchCached } from '$lib/utils/download';
import type { ArtistLinks, SongArtists } from '$lib/types';

const ARTISTS_DB_URL = 'https://opentaiko.github.io/artists_info.db3';

/** songUid → its artists and their links */
export const LoadSongArtists = async (): Promise<Record<string, SongArtists>> => {
    const SQL = await getSQL();
    const db = new SQL.Database(new Uint8Array(await fetchCached(ARTISTS_DB_URL)));
    try {
        const link = (value: unknown): string | null => (value == null ? null : String(value));

        const artistsById: Record<string, ArtistLinks> = {};
        const artistsResult = db.exec('SELECT entryId, artist, youtube, soundcloud, spotify, bandcamp, bilibili, other FROM artists');
        if (artistsResult.length > 0) {
            for (const [entryId, artist, youtube, soundcloud, spotify, bandcamp, bilibili, other] of artistsResult[0].values) {
                artistsById[String(entryId)] = {
                    artist: String(artist ?? ''),
                    youtube: link(youtube), soundcloud: link(soundcloud), spotify: link(spotify),
                    bandcamp: link(bandcamp), bilibili: link(bilibili), other: link(other)
                };
            }
        }

        const bySong: Record<string, SongArtists> = {};
        const songsResult = db.exec('SELECT songUid, artists, link FROM songs');
        if (songsResult.length > 0) {
            for (const [songUid, artistsJson, songLink] of songsResult[0].values) {
                const artistIds = JSON.parse(String(artistsJson || '[]')) as (string | number)[];
                const artists = artistIds.map((id) => artistsById[String(id)]).filter((a): a is ArtistLinks => !!a);
                bySong[String(songUid)] = { artists, link: link(songLink) };
            }
        }
        return bySong;
    } finally {
        db.close();
    }
};
