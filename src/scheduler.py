import traceback
import logging
from hydroserverpy.schemas.data_loaders import DataLoaderPostBody
from hydroserverpy.schemas.data_sources import DataSourceGetResponse
from apscheduler.schedulers.background import BackgroundScheduler
from apscheduler.triggers.cron import CronTrigger
from apscheduler.triggers.interval import IntervalTrigger
from datetime import datetime
from pytz import utc


logger = logging.getLogger('scheduler')


class HydroLoaderScheduler:

    def __init__(self, service, instance=None):
        self.scheduler = BackgroundScheduler(timezone=utc)
        self.scheduler.add_job(
            lambda: self.check_data_sources(),
            id='hydroloader-scheduler',
            trigger='interval',
            seconds=30,
            next_run_time=datetime.utcnow()
        )

        logging.getLogger('apscheduler.executors.default').setLevel(logging.WARNING)

        self.service = service
        self.instance = instance
        self.timeout = 60
        self.scheduler.start()
        self.data_loader = None

    def check_data_sources(self):
        """
        The check_data_sources function is used to check the status of all data sources associated with a given
        instance. It will iterate through each data source and call the update_data_source function for each one.

        :param self
        :return: The data_sources
        """

        try:
            success, message = self.check_data_loader(data_loader_name=self.instance)
            if success is False:
                logging.error(message)
        except Exception as e:
            logging.error(traceback.format_exc())
            logging.error(e)

        try:
            data_sources = self.service.data_loaders.list_data_sources(data_loader_id=self.data_loader.id)
            if data_sources.data:
                for data_source in data_sources.data:
                    self.update_data_source(data_source)
        except Exception as e:
            logging.error(traceback.format_exc())
            logging.error(e)

    def check_data_loader(self, data_loader_name):
        """
        The check_data_loader function checks to see if the data loader name provided by the user exists. If it does
        not, it creates a new data loader with that name. If it does exist, then it sets self.data_loader to that
        existing data loader.

        :param self
        :param data_loader_name: Identify the data loader instance
        :return: A tuple containing a boolean and a string
        """

        response = self.service.data_loaders.list()

        if response.status_code == 401:
            return False, 'Failed to login with given username and password.'

        elif response.status_code == 403:
            return False, 'The given account does not have permission to access this resource.'

        elif response.status_code != 200:
            return False, 'Failed to retrieve account HydroLoader instances.'

        data_loaders = response.data

        if data_loader_name not in [
            data_loader.name for data_loader in data_loaders
        ]:
            response = self.service.data_loaders.create(
                data_loader_body=DataLoaderPostBody(name=data_loader_name)
            )

            if response.status_code != 201:
                return False, 'Failed to register HydroLoader instance.'

            self.data_loader = response.data

        else:
            self.data_loader = next(iter([
                data_loader for data_loader in data_loaders if data_loader.name == data_loader_name
            ]))

        return True, ''

    def update_data_source(self, data_source):
        """
        The update_data_source function is called when a user updates the schedule of an existing data source.
        It checks to see if the data source has a scheduled job, and if it does not, it adds one. If there is already
        a scheduled job for that data source, then update_data_source calls update_schedule to change the schedule.

        :param self
        :param data_source: Identify the data source that is being updated
        :return: bool
        """

        scheduled_jobs = {
            scheduled_job.id: scheduled_job
            for scheduled_job in self.scheduler.get_jobs()
            if scheduled_job.id != 'hydroloader-scheduler'
        }

        if str(data_source.id) not in scheduled_jobs.keys():
            self.add_schedule(data_source)
        else:
            self.update_schedule(data_source, scheduled_jobs[str(data_source.id)])

        return True

    def add_schedule(self, data_source: DataSourceGetResponse):
        """
        The add_schedule function is used to add a schedule for the data source. The function takes in a
        DataSourceGetResponse object as an argument, which contains all the information needed to create and run
        scheduled data loading tasks.

        :param self
        :param data_source: DataSourceGetResponse: Pass the data source object to the function
        :return: None
        """
        schedule_range = {}
        if data_source.start_time:
            schedule_range['start_time'] = data_source.start_time
        if data_source.end_time:
            schedule_range['end_time'] = data_source.end_time

        if data_source.interval and data_source.interval_units:
            self.scheduler.add_job(
                lambda: self.load_data(data_source=data_source),
                IntervalTrigger(
                    timezone='UTC',
                    **{data_source.interval_units: data_source.interval}
                ),
                id=str(data_source.id),
                **schedule_range
            )
        elif data_source.crontab:
            self.scheduler.add_job(
                lambda: self.load_data(data_source=data_source),
                CronTrigger.from_crontab(data_source.crontab, timezone='UTC'),
                id=str(data_source.id),
                **schedule_range
            )

    def update_schedule(self, data_source: DataSourceGetResponse, scheduled_job):
        """
        The update_schedule function is called when a data source is updated.
        It checks if the crontab or interval has changed, and if so, removes the old job from the scheduler and adds a
        new one. If neither have changed, it does nothing.

        :param self
        :param data_source: DataSourceGetResponse: Get the data source information
        :param scheduled_job: Get the job id and trigger
        :return: None
        """

        if (
            (isinstance(scheduled_job.trigger, CronTrigger) and not data_source.crontab) or
            (isinstance(scheduled_job.trigger, IntervalTrigger) and not data_source.interval)
        ):
            self.scheduler.remove_job(scheduled_job.id)

        if isinstance(scheduled_job.trigger, CronTrigger):
            data_source_trigger = CronTrigger.from_crontab(data_source.crontab, timezone='UTC')
            data_source_trigger_value = str(data_source_trigger)
            scheduled_job_trigger_value = str(scheduled_job.trigger)
        elif isinstance(scheduled_job.trigger, IntervalTrigger):
            data_source_trigger = IntervalTrigger(
                timezone='UTC',
                **{data_source.interval_units: data_source.interval}
            )
            data_source_trigger_value = data_source_trigger.interval_length
            scheduled_job_trigger_value = scheduled_job.trigger.interval_length
        else:
            data_source_trigger_value = None
            scheduled_job_trigger_value = None

        if data_source_trigger_value != scheduled_job_trigger_value:
            self.scheduler.remove_job(scheduled_job.id)

        if not self.scheduler.get_job(scheduled_job.id) and \
                (data_source.crontab or data_source.interval):
            self.add_schedule(data_source)

    def load_data(self, data_source):
        """
        The load_data function is used to load data from a data source into the
        data warehouse. The function takes in a single argument, which is an object
        representing the data source that you want to load. This function will then
        call on the service's 'load_data' method, passing in the ID of your desired
        data source as an argument.

        :param self
        :param data_source: Identify the data source that you want to load
        :return: None
        """

        logging.info(f'Loading data source {data_source.name}')

        try:
            if data_source.paused:
                return None

            self.service.data_sources.load_data(data_source_id=data_source.id)

            logging.info(f'Finished loading data source {data_source.name}')

        except Exception as e:
            logging.error(traceback.format_exc())
            logging.error(e)
