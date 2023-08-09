import traceback
import json
import requests
import logging
import utils
from apscheduler.schedulers.background import BackgroundScheduler
from apscheduler.triggers.cron import CronTrigger
from apscheduler.triggers.interval import IntervalTrigger
from datetime import datetime
from pytz import utc
from hydroloader import HydroLoader, HydroLoaderConf


logger = logging.getLogger('scheduler')


class DataSource(HydroLoaderConf):
    id: str


class HydroLoaderScheduler:

    def __init__(self, service, auth, instance=None):
        self.scheduler = BackgroundScheduler(timezone=utc)
        self.scheduler.add_job(
            lambda: self.update_data_sources(),
            id='hydroloader-scheduler',
            trigger='interval',
            seconds=30,
            next_run_time=datetime.utcnow()
        )

        logging.getLogger('apscheduler.executors.default').setLevel(logging.WARNING)

        self.auth = auth
        self.service = service
        self.instance = instance
        self.session = requests.Session()
        self.session.auth = self.auth
        self.scheduler.start()

    def update_data_sources(self):
        """
        The update_data_sources function is used to sync local scheduled jobs with data sources registered on
        HydroServer. It first gets a list of all the data sources from HydroServer, then updates each one individually.

        :param self: Represent the instance of the class
        :return: None
        """

        # logging.info('Syncing data sources with HydroServer.')

        try:
            success, message = utils.sync_data_loader(
                url=self.service,
                name=self.instance,
                username=self.auth[0],
                password=self.auth[1]
            )
            if success is False:
                logging.error(message)
        except Exception as e:
            logging.error(traceback.format_exc())
            logging.error(e)

        try:
            data_sources = self.get_data_sources()
            for data_source in data_sources:
                self.update_data_source(data_source)
        except Exception as e:
            logging.error(traceback.format_exc())
            logging.error(e)

    def update_data_source(self, data_source):
        """
        The update_data_source function is called when a data source needs to be updated. It checks if the data source
        has an associated scheduled job, and if it does not, it adds one. If it does have an associated scheduled job,
        then the function updates the schedule for that job.

        :param self: Represent the instance of a class
        :param data_source: The data source to update
        :return: True
        """

        scheduled_jobs = {
            scheduled_job.id: scheduled_job
            for scheduled_job in self.scheduler.get_jobs()
            if scheduled_job.id != 'hydroloader-scheduler'
        }

        if data_source.id not in scheduled_jobs.keys():
            self.add_schedule(data_source)
        else:
            self.update_schedule(data_source, scheduled_jobs[data_source.id])

        return True

    def get_data_source(self, data_source_id):
        """
        The get_data_source function retrieves a data source from the HydroServer.

        :param self: Represent the instance of the class
        :param data_source_id: Identify the data source that is being requested
        :return: A datasource object
        """

        request_url = f'{self.service}/api/data-sources/{data_source_id}'
        response = self.session.get(request_url)

        if response.status_code != 200:
            raise requests.RequestException(
                f'Failed to retrieve data source {data_source_id} from HydroServer: {str(response)}'
            )

        data_source = json.loads(response.content)
        data_source = DataSource(**data_source)

        return data_source

    def get_data_sources(self):
        """
        The get_data_sources function retrieves a list of data sources from the HydroServer.

        :param self: Represent the instance of the class
        :return: A list of datasource objects
        """

        request_url = f'{self.service}/api/data-sources'
        response = self.session.get(request_url)

        if response.status_code != 200:
            raise requests.RequestException(f'Failed to retrieve data sources from HydroServer: {str(response)}')

        data_sources = json.loads(response.content)
        data_sources = [
            DataSource(**data_source) for data_source in data_sources
            if not self.instance or data_source.get('data_loader', {}).get('name') == self.instance
        ]

        return data_sources

    def update_data_source_status(self, data_source_id, data_source_status):
        """
        The update_data_source_status function updates the status of a data source on HydroServer.

        :param self: Represent the instance of the class
        :param data_source_id: The ID of the data source whose status will be updated
        :param data_source_status: The data source status
        :return: None
        """

        request_url = f'{self.service}/api/data-sources/{data_source_id}'
        response = self.session.patch(request_url, json=data_source_status)

        if response.status_code != 204:
            raise requests.RequestException(
                f'Failed to update data source status for data source {data_source_id}: {str(response)}'
            )

    def add_schedule(self, data_source):
        """
        The add_schedule function is used to add a job to the scheduler. The function takes in a data_source object as
        an argument and uses its schedule attribute to determine how often the load_data function should be called.
        The load_data function is  called with the id of the data source as an argument so that it knows which data
        source's load method to call when it runs.

        :param self: Refer to the current instance of a class
        :param data_source: The data source to be scheduled locally.
        :return: None
        """

        schedule_range = {}
        if data_source.schedule.start_time:
            schedule_range['start_time'] = data_source.schedule.start_time
        if data_source.schedule.end_time:
            schedule_range['end_time'] = data_source.schedule.end_time

        if data_source.schedule and data_source.schedule.interval and data_source.schedule.interval_units:
            self.scheduler.add_job(
                lambda: self.load_data(data_source.id),
                IntervalTrigger(
                    timezone='UTC',
                    **{data_source.schedule.interval_units: data_source.schedule.interval}
                ),
                id=data_source.id,
                **schedule_range
            )
        elif data_source.schedule and data_source.schedule.crontab:
            self.scheduler.add_job(
                lambda: self.load_data(data_source.id),
                CronTrigger.from_crontab(data_source.schedule.crontab, timezone='UTC'),
                id=data_source.id,
                **schedule_range
            )

    def update_schedule(self, data_source, scheduled_job):
        """
        The update_schedule function is called when a data source's schedule is updated. It checks to see if the new
        schedule has been set, and if so, it removes the old job from the scheduler and adds a new one with the updated
        parameters. If no schedule has been set for this data source, then it simply removes any existing jobs from the
        scheduler.

        :param self: Refer to the object itself
        :param data_source: The data source object
        :param scheduled_job: The scheduler object for the data source
        :return: None
        """

        if (
            not data_source.schedule or
            (isinstance(scheduled_job.trigger, CronTrigger) and not data_source.schedule.crontab) or
            (isinstance(scheduled_job.trigger, IntervalTrigger) and not data_source.schedule.interval)
        ):
            self.scheduler.remove_job(scheduled_job.id)

        if isinstance(scheduled_job.trigger, CronTrigger):
            data_source_trigger = CronTrigger.from_crontab(data_source.schedule.crontab, timezone='UTC')
            data_source_trigger_value = str(data_source_trigger)
            scheduled_job_trigger_value = str(scheduled_job.trigger)
        elif isinstance(scheduled_job.trigger, IntervalTrigger):
            data_source_trigger = IntervalTrigger(
                timezone='UTC',
                **{data_source.schedule.interval_units: data_source.schedule.interval}
            )
            data_source_trigger_value = data_source_trigger.interval_length
            scheduled_job_trigger_value = scheduled_job.trigger.interval_length
        else:
            data_source_trigger_value = None
            scheduled_job_trigger_value = None

        if data_source_trigger_value != scheduled_job_trigger_value:
            self.scheduler.remove_job(scheduled_job.id)

        if not self.scheduler.get_job(scheduled_job.id) and data_source.schedule and \
                (data_source.schedule.crontab or data_source.schedule.interval):
            self.add_schedule(data_source)

    def load_data(self, data_source_id):
        """
        The load_data function is the main function of the HydroLoader class.
        It takes a data_source_id as an argument and uses it to retrieve a data source from
        the database. It then checks if that data source has been paused, and if not, it
        continues loading by calling the sync_datastreams method of HydroLoader. The results
        of this call are used to update the status of this particular data source in our database.

        :param self: Represent the instance of the class
        :param data_source_id: Identify the data source to be loaded
        :return: A dictionary of data source status
        """

        logging.info(f'Loading data source {data_source_id}')

        try:
            data_source = self.get_data_source(data_source_id)

            if not data_source:
                return None

            continue_loading = self.update_data_source(data_source)

            if not continue_loading:
                return None

            if data_source.schedule.paused:
                return None

            loader = HydroLoader(
                conf=data_source,
                auth=self.auth,
                service=f'{self.service}/sensorthings/v1.1'
            )

            results = loader.sync_datastreams()

            data_source_status = {
                'data_source_thru': str(results.get('data_thru')) if results.get('data_thru') else None,
                'last_sync_successful': results.get('success'),
                'last_sync_message': results.get('message'),
                'last_synced': str(datetime.utcnow()),
                'next_sync': str(self.scheduler.get_job(data_source_id).next_run_time)
            }

        except Exception as e:
            logging.error(traceback.format_exc())
            logging.error(e)
            data_source_status = {
                'data_source_thru': None,
                'last_sync_successful': False,
                'last_sync_message': str(e),
                'last_synced': str(datetime.utcnow()),
                'next_sync': str(self.scheduler.get_job(data_source_id).next_run_time)
            }

        try:
            self.update_data_source_status(
                data_source_id=data_source_id,
                data_source_status=data_source_status
            )
        except Exception as e:
            logging.error(traceback.format_exc())
            logging.error(e)
